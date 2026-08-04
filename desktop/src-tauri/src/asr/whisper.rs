use live_translator_core::TokenPiece;
use std::error::Error;
use std::fmt;
use std::path::Path;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

#[derive(Debug, Clone)]
pub struct WhisperEngineConfig {
    pub n_threads: i32,
    pub use_gpu: bool,
    pub flash_attention: bool,
}

impl Default for WhisperEngineConfig {
    fn default() -> Self {
        let n_threads = std::thread::available_parallelism()
            .map(|value| value.get().min(4) as i32)
            .unwrap_or(1);
        Self {
            n_threads,
            use_gpu: cfg!(target_os = "macos"),
            flash_attention: true,
        }
    }
}

#[derive(Debug)]
pub enum WhisperEngineError {
    Model(String),
    State(String),
    Decode(String),
    Token(String),
}

impl fmt::Display for WhisperEngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Model(message) => write!(formatter, "failed to load Whisper model: {message}"),
            Self::State(message) => write!(formatter, "failed to create Whisper state: {message}"),
            Self::Decode(message) => write!(formatter, "Whisper decode failed: {message}"),
            Self::Token(message) => write!(formatter, "failed to read Whisper token: {message}"),
        }
    }
}

impl Error for WhisperEngineError {}

/// A long-lived whisper.cpp context and decoding state. Loading the model is
/// intentionally separated from `transcribe` so live revisions do not relaunch
/// a process or reload weights.
pub struct WhisperEngine {
    context: WhisperContext,
    state: WhisperState,
    config: WhisperEngineConfig,
}

impl WhisperEngine {
    pub fn load(
        model_path: impl AsRef<Path>,
        config: WhisperEngineConfig,
    ) -> Result<Self, WhisperEngineError> {
        whisper_rs::install_logging_hooks();
        let mut context_params = WhisperContextParameters::default();
        context_params
            .use_gpu(config.use_gpu)
            .flash_attn(config.flash_attention);
        let context = WhisperContext::new_with_params(model_path, context_params)
            .map_err(|error| WhisperEngineError::Model(error.to_string()))?;
        let state = context
            .create_state()
            .map_err(|error| WhisperEngineError::State(error.to_string()))?;

        Ok(Self {
            context,
            state,
            config,
        })
    }

    pub fn transcribe(&mut self, samples: &[f32]) -> Result<Vec<TokenPiece>, WhisperEngineError> {
        let mut params =
            FullParams::new(SamplingStrategy::BeamSearch {
                beam_size: 5,
                patience: 1.0,
            });
        params.set_language(Some("ja"));
        params.set_translate(false);
        params.set_n_threads(self.config.n_threads);
        params.set_no_context(false);
        // A short Japanese prompt anchors the model and reduces hallucinated
        // filler on fast, overlapping dialogue.
        params.set_initial_prompt("これは日本語の字幕です。");
        params.set_single_segment(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_temperature_inc(0.0);

        self.state
            .full(params, samples)
            .map_err(|error| WhisperEngineError::Decode(error.to_string()))?;

        self.collect_text_tokens()
    }

    fn collect_text_tokens(&self) -> Result<Vec<TokenPiece>, WhisperEngineError> {
        let end_of_text = self.context.token_eot();
        let mut pieces = Vec::new();
        let mut incomplete_utf8 = Vec::new();

        for segment in self.state.as_iter() {
            for token_index in 0..segment.n_tokens() {
                let token = segment
                    .get_token(token_index)
                    .expect("token index came from whisper.cpp");
                let id = token.token_id();
                if id >= end_of_text {
                    continue;
                }

                let bytes = token
                    .to_bytes()
                    .map_err(|error| WhisperEngineError::Token(error.to_string()))?;
                incomplete_utf8.extend_from_slice(bytes);
                let text = take_valid_utf8(&mut incomplete_utf8);
                pieces.push(TokenPiece { id, text });
            }
        }

        if !incomplete_utf8.is_empty() {
            let tail = String::from_utf8_lossy(&incomplete_utf8);
            if let Some(piece) = pieces.last_mut() {
                piece.text.push_str(&tail);
            }
        }

        Ok(pieces)
    }
}

fn take_valid_utf8(buffer: &mut Vec<u8>) -> String {
    let (valid_len, starts_with_invalid_byte) = match std::str::from_utf8(buffer) {
        Ok(_) => (buffer.len(), false),
        Err(error) => (
            error.valid_up_to(),
            error.valid_up_to() == 0 && error.error_len().is_some(),
        ),
    };
    if starts_with_invalid_byte {
        return String::from_utf8_lossy(&std::mem::take(buffer)).into_owned();
    }
    if valid_len == 0 {
        return String::new();
    }

    let valid = String::from_utf8(buffer.drain(..valid_len).collect())
        .expect("valid UTF-8 prefix was already checked");

    if let Err(error) = std::str::from_utf8(buffer) {
        if error.error_len().is_some() {
            valid + &String::from_utf8_lossy(&std::mem::take(buffer))
        } else {
            valid
        }
    } else {
        valid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_fragments_are_reassembled_without_replacement_characters() {
        let character = "日".as_bytes();
        let mut pending = Vec::new();

        pending.extend_from_slice(&character[..2]);
        assert_eq!(take_valid_utf8(&mut pending), "");
        pending.extend_from_slice(&character[2..]);
        assert_eq!(take_valid_utf8(&mut pending), "日");
        assert!(pending.is_empty());
    }

    #[test]
    fn valid_prefix_is_emitted_while_incomplete_suffix_is_retained() {
        let mut pending = "A日".as_bytes()[..3].to_vec();

        assert_eq!(take_valid_utf8(&mut pending), "A");
        pending.extend_from_slice(&"日".as_bytes()[2..]);
        assert_eq!(take_valid_utf8(&mut pending), "日");
    }

    #[test]
    fn invalid_utf8_is_consumed_instead_of_blocking_later_tokens() {
        let mut pending = vec![0xff];

        assert_eq!(take_valid_utf8(&mut pending), "�");
        assert!(pending.is_empty());
    }
}
