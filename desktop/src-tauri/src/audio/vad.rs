use live_translator_core::WHISPER_SAMPLE_RATE;
use std::path::Path;
use whisper_rs::{WhisperVadContext, WhisperVadContextParams, WhisperVadParams};

const DEFAULT_LOOKBACK_MS: u64 = 4_000;
const DEFAULT_ENDPOINT_GUARD_MS: u64 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VadObservation {
    pub speech_seen: bool,
    pub end_of_speech: bool,
    pub last_speech_end_ms: Option<u64>,
}

pub struct SileroVad {
    context: WhisperVadContext,
    params: WhisperVadParams,
    lookback_samples: usize,
    endpoint_guard_ms: u64,
    speech_seen: bool,
}

impl SileroVad {
    pub fn load(model_path: impl AsRef<Path>) -> Result<Self, String> {
        let model_path = model_path.as_ref();
        let model_path = model_path
            .to_str()
            .ok_or_else(|| "VAD model path is not valid UTF-8".to_owned())?;
        let mut context_params = WhisperVadContextParams::default();
        context_params.set_n_threads(1);
        context_params.set_use_gpu(false);
        let context = WhisperVadContext::new(model_path, context_params)
            .map_err(|error| format!("failed to load Silero VAD: {error}"))?;

        let mut params = WhisperVadParams::default();
        params.set_threshold(0.5);
        params.set_min_speech_duration(160);
        params.set_min_silence_duration(500);
        params.set_max_speech_duration(12.0);
        params.set_speech_pad(80);
        params.set_samples_overlap(0.1);

        Ok(Self {
            context,
            params,
            lookback_samples: milliseconds_to_samples(DEFAULT_LOOKBACK_MS),
            endpoint_guard_ms: DEFAULT_ENDPOINT_GUARD_MS,
            speech_seen: false,
        })
    }

    pub fn observe(&mut self, samples: &[f32]) -> Result<VadObservation, String> {
        let tail_start = samples.len().saturating_sub(self.lookback_samples);
        let tail = &samples[tail_start..];
        if tail.is_empty() {
            return Ok(VadObservation {
                speech_seen: self.speech_seen,
                end_of_speech: false,
                last_speech_end_ms: None,
            });
        }

        let segments = self
            .context
            .segments_from_samples(self.params, tail)
            .map_err(|error| format!("Silero VAD inference failed: {error}"))?;
        let last_speech_end_ms = if segments.num_segments() > 0 {
            segments
                .get_segment_end_timestamp(segments.num_segments() - 1)
                .map(centiseconds_to_milliseconds)
        } else {
            None
        };
        let speech_in_window = last_speech_end_ms.is_some();
        self.speech_seen |= speech_in_window;
        let window_duration_ms = samples_to_milliseconds(tail.len());
        let end_of_speech = classify_endpoint(
            self.speech_seen,
            window_duration_ms,
            last_speech_end_ms,
            self.endpoint_guard_ms,
        );

        Ok(VadObservation {
            speech_seen: self.speech_seen,
            end_of_speech,
            last_speech_end_ms,
        })
    }

    pub fn reset(&mut self) {
        self.speech_seen = false;
    }
}

fn classify_endpoint(
    speech_seen: bool,
    window_duration_ms: u64,
    last_speech_end_ms: Option<u64>,
    endpoint_guard_ms: u64,
) -> bool {
    if !speech_seen {
        return false;
    }
    match last_speech_end_ms {
        Some(end_ms) => window_duration_ms.saturating_sub(end_ms) >= endpoint_guard_ms,
        None => true,
    }
}

fn milliseconds_to_samples(milliseconds: u64) -> usize {
    (milliseconds * u64::from(WHISPER_SAMPLE_RATE) / 1_000) as usize
}

fn samples_to_milliseconds(samples: usize) -> u64 {
    samples as u64 * 1_000 / u64::from(WHISPER_SAMPLE_RATE)
}

fn centiseconds_to_milliseconds(centiseconds: f32) -> u64 {
    (centiseconds.max(0.0) * 10.0).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_requires_prior_speech() {
        assert!(!classify_endpoint(false, 4_000, None, 250));
    }

    #[test]
    fn ongoing_speech_at_window_tail_does_not_end_utterance() {
        assert!(!classify_endpoint(true, 4_000, Some(3_900), 250));
    }

    #[test]
    fn trailing_silence_ends_utterance() {
        assert!(classify_endpoint(true, 4_000, Some(3_500), 250));
        assert!(classify_endpoint(true, 4_000, None, 250));
    }

    #[test]
    fn time_conversions_match_whisper_sample_rate() {
        assert_eq!(milliseconds_to_samples(20), 320);
        assert_eq!(samples_to_milliseconds(32_000), 2_000);
        assert_eq!(centiseconds_to_milliseconds(123.0), 1_230);
    }
}
