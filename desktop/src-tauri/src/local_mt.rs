use llama_cpp::standard_sampler::{SamplerStage, StandardSampler};
use llama_cpp::{LlamaModel, LlamaParams, SessionParams};
use std::path::Path;

const MAX_OUTPUT_TOKENS: usize = 512;
const HY_MT_PROMPT: &str =
    "Translate the following segment into Chinese, without additional explanation.\n\n";

/// Local Japanese-to-Chinese translation via the Tencent Hy-MT1.5 GGUF model
/// running on llama.cpp. Completely offline; the model is a 1.1 GB Q4_K_M
/// quantization stored under the model directory.
#[derive(Clone)]
pub struct LocalMtTranslator {
    model: LlamaModel,
    sampler: StandardSampler,
    prompt: String,
}

impl LocalMtTranslator {
    pub fn new(model_dir: impl AsRef<Path>) -> Result<Self, String> {
        let path = model_dir.as_ref().join(HY_MT_MODEL_FILE).to_path_buf();
        if !path.is_file() {
            return Err(format!(
                "Hy-MT translation model is missing: {}",
                path.display()
            ));
        }
        let model = LlamaModel::load_from_file(&path, LlamaParams::default())
            .map_err(|error| format!("failed to load Hy-MT model: {error}"))?;
        // Recommended inference parameters from the Hy-MT model card.
        let sampler = StandardSampler::new_softmax(
            vec![
                SamplerStage::Temperature(0.7),
                SamplerStage::TopK(20),
                SamplerStage::TopP(0.6),
                SamplerStage::RepetitionPenalty {
                    repetition_penalty: 1.05,
                    frequency_penalty: 0.0,
                    presence_penalty: 0.0,
                    last_n: -1,
                },
            ],
            1,
        );
        let prompt = HY_MT_PROMPT.to_owned();
        Ok(Self {
            model,
            sampler,
            prompt,
        })
    }

    pub fn translate(&self, text_ja: &str) -> Result<String, String> {
        let input = format!("{}{}", self.prompt, text_ja);
        let mut session = self
            .model
            .create_session(SessionParams::default())
            .map_err(|error| format!("failed to create Hy-MT session: {error}"))?;
        session
            .advance_context(input.as_bytes())
            .map_err(|error| format!("failed to advance Hy-MT context: {error}"))?;

        let completion = session
            .start_completing_with(self.sampler.clone(), MAX_OUTPUT_TOKENS)
            .map_err(|error| format!("failed to start Hy-MT completion: {error}"))?
            .into_string();

        let translated = completion.trim();
        if translated.is_empty() {
            return Err("Hy-MT returned no translation".to_owned());
        }
        Ok(translated.to_owned())
    }
}

pub const HY_MT_MODEL_FILE: &str = "hymt/HY-MT1.5-1.8B-Q4_K_M.gguf";
