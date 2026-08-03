use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const TRANSLATION_EVENT: &str = "subtitle:translation";

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_PROMPT: &str =
    "Translate Japanese subtitles into natural Simplified Chinese. Return only the translation, without explanations or quotation marks.";

#[derive(Debug, Clone)]
pub struct TranslationConfig {
    endpoint: String,
    api_key: String,
    model: String,
    system_prompt: String,
    timeout: Duration,
}

impl TranslationConfig {
    pub fn from_env() -> Option<Self> {
        let api_key = env::var("LIVE_TRANSLATOR_API_KEY").ok()?;
        let api_key = api_key.trim().to_owned();
        if api_key.is_empty() {
            return None;
        }

        let endpoint = env::var("LIVE_TRANSLATOR_API_URL")
            .unwrap_or_else(|_| DEFAULT_ENDPOINT.to_owned())
            .trim()
            .to_owned();
        let model = env::var("LIVE_TRANSLATOR_MODEL")
            .unwrap_or_else(|_| DEFAULT_MODEL.to_owned())
            .trim()
            .to_owned();
        let system_prompt = env::var("LIVE_TRANSLATOR_SYSTEM_PROMPT")
            .unwrap_or_else(|_| DEFAULT_PROMPT.to_owned())
            .trim()
            .to_owned();
        let timeout_ms = env::var("LIVE_TRANSLATOR_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(8_000)
            .clamp(1_000, 60_000);

        if endpoint.is_empty() || model.is_empty() || system_prompt.is_empty() {
            return None;
        }

        Some(Self {
            endpoint,
            api_key,
            model,
            system_prompt,
            timeout: Duration::from_millis(timeout_ms),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TranslationJob {
    pub segment_id: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text_ja: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TranslationPayload {
    pub segment_id: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text_ja: String,
    pub text_zh: String,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: [ChatMessage<'a>; 2],
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
}

#[derive(Debug, Clone)]
struct OpenAiCompatibleTranslator {
    client: Client,
    config: TranslationConfig,
}

impl OpenAiCompatibleTranslator {
    fn new(config: TranslationConfig) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|error| format!("failed to build translation client: {error}"))?;
        Ok(Self { client, config })
    }

    fn translate(&self, text_ja: &str) -> Result<String, String> {
        let request = ChatCompletionRequest {
            model: &self.config.model,
            messages: [
                ChatMessage {
                    role: "system",
                    content: &self.config.system_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: text_ja,
                },
            ],
            temperature: 0.0,
            stream: false,
        };
        let response = self
            .client
            .post(&self.config.endpoint)
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .map_err(|error| format!("translation request failed: {error}"))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|error| format!("failed to read translation response: {error}"))?;
        parse_translation_response(status, &body)
    }
}

fn parse_translation_response(status: reqwest::StatusCode, body: &str) -> Result<String, String> {
    if !status.is_success() {
        return Err(format!("translation API returned {status}"));
    }

    let response: ChatCompletionResponse = serde_json::from_str(body)
        .map_err(|error| format!("invalid translation response: {error}"))?;
    let text = response
        .choices
        .first()
        .map(|choice| choice.message.content.trim())
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "translation response has no text".to_owned())?;
    Ok(text.to_owned())
}

/// Runs API calls away from the ASR thread. A small bounded queue keeps a slow
/// provider from building unbounded translation backlog during a live stream.
pub struct TranslationWorker {
    queue: Option<Arc<TranslationQueue>>,
    join: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct TranslationQueueState {
    latest: Option<TranslationJob>,
    closed: bool,
}

#[derive(Default)]
struct TranslationQueue {
    state: Mutex<TranslationQueueState>,
    wake: Condvar,
}

impl TranslationWorker {
    pub fn start(app: AppHandle, stop: Arc<AtomicBool>) -> Self {
        let Some(config) = TranslationConfig::from_env() else {
            return Self {
                queue: None,
                join: None,
            };
        };

        let Ok(translator) = OpenAiCompatibleTranslator::new(config) else {
            return Self {
                queue: None,
                join: None,
            };
        };
        let queue = Arc::new(TranslationQueue::default());
        let worker_queue = Arc::clone(&queue);
        let join = thread::Builder::new()
            .name("live-translator-translation".to_owned())
            .spawn(move || run_worker(app, stop, translator, worker_queue));
        match join {
            Ok(join) => Self {
                queue: Some(queue),
                join: Some(join),
            },
            Err(_) => Self {
                queue: None,
                join: None,
            },
        }
    }

    pub fn submit(&self, job: TranslationJob) {
        let Some(queue) = &self.queue else {
            return;
        };
        let Ok(mut state) = queue.state.lock() else {
            return;
        };
        if state.closed {
            return;
        }
        state.latest = Some(job);
        queue.wake.notify_one();
    }
}

impl Drop for TranslationWorker {
    fn drop(&mut self) {
        if let Some(queue) = self.queue.take() {
            if let Ok(mut state) = queue.state.lock() {
                state.closed = true;
                queue.wake.notify_one();
            }
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_worker(
    app: AppHandle,
    stop: Arc<AtomicBool>,
    translator: OpenAiCompatibleTranslator,
    queue: Arc<TranslationQueue>,
) {
    loop {
        let job = {
            let Ok(mut state) = queue.state.lock() else {
                break;
            };
            while state.latest.is_none() && !state.closed && !stop.load(Ordering::Acquire) {
                let Ok(next) = queue.wake.wait(state) else {
                    return;
                };
                state = next;
            }
            if stop.load(Ordering::Acquire) || (state.closed && state.latest.is_none()) {
                break;
            }
            let Some(job) = state.latest.take() else {
                continue;
            };
            job
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        let Ok(text_zh) = translator.translate(&job.text_ja) else {
            continue;
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        let _ = app.emit(
            TRANSLATION_EVENT,
            TranslationPayload {
                segment_id: job.segment_id,
                start_ms: job.start_ms,
                end_ms: job.end_ms,
                text_ja: job.text_ja,
                text_zh,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_chat_completion_response() {
        let body = r#"{"choices":[{"message":{"content":"  今日は晴れです。  "}}]}"#;

        assert_eq!(
            parse_translation_response(reqwest::StatusCode::OK, body).unwrap(),
            "今日は晴れです。"
        );
    }

    #[test]
    fn rejects_unsuccessful_status_without_exposing_response_body() {
        let error = parse_translation_response(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error":{"message":"sensitive provider detail"}}"#,
        )
        .unwrap_err();

        assert_eq!(error, "translation API returned 401 Unauthorized");
        assert!(!error.contains("sensitive"));
    }

    #[test]
    fn pending_translation_is_replaced_by_the_latest_revision() {
        let queue = Arc::new(TranslationQueue::default());
        let worker = TranslationWorker {
            queue: Some(Arc::clone(&queue)),
            join: None,
        };
        let job = |segment_id| TranslationJob {
            segment_id,
            start_ms: 0,
            end_ms: segment_id * 1_000,
            text_ja: format!("segment {segment_id}"),
        };

        worker.submit(job(1));
        worker.submit(job(2));

        let state = queue.state.lock().unwrap();
        assert_eq!(state.latest.as_ref().unwrap().segment_id, 2);
    }
}
