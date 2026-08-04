use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const TRANSLATION_EVENT: &str = "subtitle:translation";

const DEFAULT_OPENAI_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_DEEPL_ENDPOINT: &str = "https://api-free.deepl.com/v2/translate";
const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_PROMPT: &str =
    "Translate Japanese subtitles into natural Simplified Chinese. Return only the translation, without explanations or quotation marks.";
const DEFAULT_TIMEOUT_MS: u64 = 8_000;
const MINIMUM_TIMEOUT_MS: u64 = 1_000;
const MAXIMUM_TIMEOUT_MS: u64 = 60_000;
const OPENAI_COMPATIBLE_PROVIDER: &str = "openai-compatible";
const DEEPL_PROVIDER: &str = "deepl";
const LOCAL_MT_PROVIDER: &str = "local";
const TRANSLATION_SETTINGS_FILE: &str = "translation-settings.json";
const CREDENTIAL_SERVICE: &str = "com.mon3tr.live-translator.translation";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranslationProvider {
    OpenAiCompatible,
    DeepL,
    LocalMt,
}

impl TranslationProvider {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim() {
            OPENAI_COMPATIBLE_PROVIDER => Ok(Self::OpenAiCompatible),
            DEEPL_PROVIDER => Ok(Self::DeepL),
            LOCAL_MT_PROVIDER => Ok(Self::LocalMt),
            _ => Err("the selected translation provider is not supported".to_owned()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => OPENAI_COMPATIBLE_PROVIDER,
            Self::DeepL => DEEPL_PROVIDER,
            Self::LocalMt => LOCAL_MT_PROVIDER,
        }
    }

    fn default_endpoint(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => DEFAULT_OPENAI_ENDPOINT,
            Self::DeepL => DEFAULT_DEEPL_ENDPOINT,
            Self::LocalMt => "",
        }
    }

    fn credential_account(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai-compatible-api-key",
            Self::DeepL => "deepl-api-key",
            Self::LocalMt => "local-mt",
        }
    }

    fn environment_api_key_name(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "LIVE_TRANSLATOR_API_KEY",
            Self::DeepL => "LIVE_TRANSLATOR_DEEPL_API_KEY",
            Self::LocalMt => "LIVE_TRANSLATOR_LOCAL_MT",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTranslationSettings {
    provider: String,
    endpoint: String,
    model: String,
    system_prompt: String,
    timeout_ms: u64,
}

impl StoredTranslationSettings {
    fn defaults_from_environment() -> Self {
        Self {
            provider: OPENAI_COMPATIBLE_PROVIDER.to_owned(),
            endpoint: environment_value(
                "LIVE_TRANSLATOR_API_URL",
                TranslationProvider::OpenAiCompatible.default_endpoint(),
            ),
            model: environment_value("LIVE_TRANSLATOR_MODEL", DEFAULT_MODEL),
            system_prompt: environment_value("LIVE_TRANSLATOR_SYSTEM_PROMPT", DEFAULT_PROMPT),
            timeout_ms: env::var("LIVE_TRANSLATOR_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(DEFAULT_TIMEOUT_MS)
                .clamp(MINIMUM_TIMEOUT_MS, MAXIMUM_TIMEOUT_MS),
        }
    }

    fn normalized(mut self) -> Result<Self, String> {
        self.provider = self.provider.trim().to_owned();
        self.endpoint = self.endpoint.trim().to_owned();
        self.model = self.model.trim().to_owned();
        self.system_prompt = self.system_prompt.trim().to_owned();

        let provider = TranslationProvider::parse(&self.provider)?;
        self.provider = provider.as_str().to_owned();

        // Local providers run entirely offline and need no endpoint.
        if provider != TranslationProvider::LocalMt {
            if self.endpoint.is_empty() {
                return Err("translation endpoint must not be empty".to_owned());
            }
            let endpoint = reqwest::Url::parse(&self.endpoint)
                .map_err(|_| "translation endpoint must be a valid URL".to_owned())?;
            if endpoint.scheme() != "https" && endpoint.scheme() != "http" {
                return Err("translation endpoint must use HTTP or HTTPS".to_owned());
            }
        }
        if provider == TranslationProvider::OpenAiCompatible
            && (self.model.is_empty() || self.system_prompt.is_empty())
        {
            return Err("translation model and prompt must not be empty".to_owned());
        }
        if !(MINIMUM_TIMEOUT_MS..=MAXIMUM_TIMEOUT_MS).contains(&self.timeout_ms) {
            return Err(format!(
                "translation timeout must be between {MINIMUM_TIMEOUT_MS} and {MAXIMUM_TIMEOUT_MS} ms"
            ));
        }
        Ok(self)
    }

    fn provider_kind(&self) -> Result<TranslationProvider, String> {
        TranslationProvider::parse(&self.provider)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSettingsPayload {
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub api_key_configured: bool,
    pub api_key_stored_in_credential_store: bool,
    pub credential_store_available: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSettingsUpdate {
    provider: String,
    endpoint: String,
    model: String,
    api_key: Option<String>,
}

fn environment_value(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

fn is_localhost(endpoint: &str) -> bool {
    reqwest::Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| {
            host == "localhost" || host == "127.0.0.1" || host == "::1"
        })
}

fn translation_settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(TRANSLATION_SETTINGS_FILE))
        .map_err(|_| "unable to locate the application settings directory".to_owned())
}

fn load_stored_settings(app: &AppHandle) -> Result<StoredTranslationSettings, String> {
    let path = translation_settings_path(app)?;
    let contents = match fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return StoredTranslationSettings::defaults_from_environment().normalized();
        }
        Err(_) => return Err("unable to read translation settings".to_owned()),
    };
    serde_json::from_slice::<StoredTranslationSettings>(&contents)
        .map_err(|_| "translation settings are invalid".to_owned())?
        .normalized()
}

fn save_stored_settings(
    app: &AppHandle,
    settings: &StoredTranslationSettings,
) -> Result<(), String> {
    let path = translation_settings_path(app)?;
    let directory = path
        .parent()
        .ok_or_else(|| "unable to locate the application settings directory".to_owned())?;
    fs::create_dir_all(directory)
        .map_err(|_| "unable to create the application settings directory".to_owned())?;
    let contents = serde_json::to_vec_pretty(settings)
        .map_err(|_| "unable to encode translation settings".to_owned())?;
    fs::write(path, contents).map_err(|_| "unable to save translation settings".to_owned())
}

fn credential_entry(provider: TranslationProvider) -> Result<keyring::Entry, String> {
    keyring::Entry::new(CREDENTIAL_SERVICE, provider.credential_account())
        .map_err(|_| "the OS credential store is unavailable".to_owned())
}

fn credential_store_api_key(provider: TranslationProvider) -> Option<String> {
    credential_entry(provider)
        .ok()?
        .get_password()
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn environment_api_key(provider: TranslationProvider) -> Option<String> {
    env::var(provider.environment_api_key_name())
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn resolved_api_key(provider: TranslationProvider) -> Option<String> {
    credential_store_api_key(provider).or_else(|| environment_api_key(provider))
}

fn set_credential_store_api_key(
    provider: TranslationProvider,
    api_key: &str,
) -> Result<(), String> {
    credential_entry(provider)?
        .set_password(api_key)
        .map_err(|_| "unable to store the API key in the OS credential store".to_owned())
}

fn delete_credential_store_api_key(provider: TranslationProvider) -> Result<(), String> {
    let entry = credential_entry(provider)?;
    match entry.get_password() {
        Ok(_) => entry
            .delete_credential()
            .map_err(|_| "unable to remove the API key from the OS credential store".to_owned()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("unable to read the API key from the OS credential store".to_owned()),
    }
}

fn settings_payload(settings: StoredTranslationSettings) -> TranslationSettingsPayload {
    let provider = settings
        .provider_kind()
        .expect("translation settings must be normalized before serialization");
    let api_key_stored_in_credential_store = credential_store_api_key(provider).is_some();
    TranslationSettingsPayload {
        provider: settings.provider,
        endpoint: settings.endpoint,
        model: settings.model,
        api_key_configured: api_key_stored_in_credential_store
            || environment_api_key(provider).is_some(),
        api_key_stored_in_credential_store,
        credential_store_available: keyring::Entry::store_status().is_ok(),
    }
}

#[tauri::command]
pub fn get_translation_settings(app: AppHandle) -> Result<TranslationSettingsPayload, String> {
    load_stored_settings(&app).map(settings_payload)
}

#[tauri::command]
pub fn save_translation_settings(
    app: AppHandle,
    update: TranslationSettingsUpdate,
) -> Result<TranslationSettingsPayload, String> {
    let current = load_stored_settings(&app)?;
    let settings = StoredTranslationSettings {
        provider: update.provider,
        endpoint: update.endpoint,
        model: update.model,
        system_prompt: current.system_prompt,
        timeout_ms: current.timeout_ms,
    }
    .normalized()?;
    let provider = settings.provider_kind()?;

    if let Some(api_key) = update.api_key {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            delete_credential_store_api_key(provider)?;
        } else {
            set_credential_store_api_key(provider, api_key)?;
        }
    }

    save_stored_settings(&app, &settings)?;
    Ok(settings_payload(settings))
}

#[derive(Debug, Clone)]
pub struct TranslationConfig {
    provider: TranslationProvider,
    endpoint: String,
    api_key: String,
    model: String,
    system_prompt: String,
    timeout: Duration,
    local_model_dir: Option<PathBuf>,
}

impl TranslationConfig {
    pub fn load(app: &AppHandle) -> Option<Self> {
        let settings = load_stored_settings(app).ok()?;
        let provider = settings.provider_kind().ok()?;
        // Local providers (self-hosted DeepLX, Hy-MT) need no credential.
        let api_key = match provider {
            TranslationProvider::LocalMt => String::new(),
            TranslationProvider::DeepL if is_localhost(&settings.endpoint) => String::new(),
            _ => resolved_api_key(provider)?,
        };

        let local_model_dir = (provider == TranslationProvider::LocalMt)
            .then(|| crate::model_directory(app));

        Some(Self {
            provider,
            endpoint: settings.endpoint,
            api_key,
            model: settings.model,
            system_prompt: settings.system_prompt,
            timeout: Duration::from_millis(settings.timeout_ms),
            local_model_dir,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TranslationJob {
    pub segment_id: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text_ja: String,
    pub submitted_wall_ms: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TranslationPayload {
    pub segment_id: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text_ja: String,
    pub text_zh: String,
    pub latency_ms: u64,
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

#[derive(Debug, Serialize)]
struct DeepLRequest<'a> {
    text: [&'a str; 1],
    target_lang: &'static str,
    source_lang: &'static str,
}

#[derive(Debug, Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Debug, Deserialize)]
struct DeepLTranslation {
    text: String,
}

#[derive(Debug, Clone)]
struct DeepLTranslator {
    client: Client,
    config: TranslationConfig,
}

impl DeepLTranslator {
    fn new(config: TranslationConfig) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|error| format!("failed to build translation client: {error}"))?;
        Ok(Self { client, config })
    }

    fn translate(&self, text_ja: &str) -> Result<String, String> {
        let request = DeepLRequest {
            text: [text_ja],
            target_lang: "ZH",
            source_lang: "JA",
        };
        let response = self
            .client
            .post(&self.config.endpoint)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("DeepL-Auth-Key {}", self.config.api_key),
            )
            .json(&request)
            .send()
            .map_err(|error| format!("translation request failed: {error}"))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|error| format!("failed to read translation response: {error}"))?;
        parse_deepl_response(status, &body)
    }
}

#[derive(Clone)]
enum TranslationClient {
    OpenAi(OpenAiCompatibleTranslator),
    DeepL(DeepLTranslator),
    LocalMt(crate::local_mt::LocalMtTranslator),
}

impl TranslationClient {
    fn new(config: TranslationConfig) -> Result<Self, String> {
        match config.provider {
            TranslationProvider::OpenAiCompatible => {
                OpenAiCompatibleTranslator::new(config).map(Self::OpenAi)
            }
            TranslationProvider::DeepL => DeepLTranslator::new(config).map(Self::DeepL),
            TranslationProvider::LocalMt => {
                let model_dir = config
                    .local_model_dir
                    .ok_or_else(|| "local translation model directory is missing".to_owned())?;
                crate::local_mt::LocalMtTranslator::new(model_dir).map(Self::LocalMt)
            }
        }
    }

    fn translate(&self, text_ja: &str) -> Result<String, String> {
        match self {
            Self::OpenAi(translator) => translator.translate(text_ja),
            Self::DeepL(translator) => translator.translate(text_ja),
            Self::LocalMt(translator) => translator.translate(text_ja),
        }
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

fn parse_deepl_response(status: reqwest::StatusCode, body: &str) -> Result<String, String> {
    if !status.is_success() {
        return Err(format!("translation API returned {status}"));
    }

    let response: DeepLResponse =
        serde_json::from_str(body).map_err(|error| format!("invalid DeepL response: {error}"))?;
    response
        .translations
        .first()
        .map(|translation| translation.text.trim())
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "DeepL response has no text".to_owned())
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
        let Some(config) = TranslationConfig::load(&app) else {
            return Self {
                queue: None,
                join: None,
            };
        };

        let Ok(translator) = TranslationClient::new(config) else {
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
    translator: TranslationClient,
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
        let translate_started = Instant::now();
        let Ok(text_zh) = translator.translate(&job.text_ja) else {
            continue;
        };
        let latency_ms = translate_started.elapsed().as_millis() as u64;
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
                latency_ms,
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
    fn parses_deepl_translation_response() {
        let body =
            r#"{"translations":[{"detected_source_language":"JA","text":" 今日は晴れです。 "}]}"#;

        assert_eq!(
            parse_deepl_response(reqwest::StatusCode::OK, body).unwrap(),
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
    fn normalizes_openai_compatible_provider_settings() {
        let settings = StoredTranslationSettings {
            provider: " openai-compatible ".to_owned(),
            endpoint: " https://gateway.example.test/v1/chat/completions ".to_owned(),
            model: " test-model ".to_owned(),
            system_prompt: " translate ".to_owned(),
            timeout_ms: 8_000,
        }
        .normalized()
        .unwrap();

        assert_eq!(settings.provider, OPENAI_COMPATIBLE_PROVIDER);
        assert_eq!(
            settings.endpoint,
            "https://gateway.example.test/v1/chat/completions"
        );
        assert_eq!(settings.model, "test-model");
    }

    #[test]
    fn accepts_deepl_provider_settings_without_a_model() {
        let settings = StoredTranslationSettings {
            provider: DEEPL_PROVIDER.to_owned(),
            endpoint: " https://api-free.deepl.com/v2/translate ".to_owned(),
            model: String::new(),
            system_prompt: String::new(),
            timeout_ms: 8_000,
        }
        .normalized()
        .unwrap();

        assert_eq!(settings.provider, DEEPL_PROVIDER);
        assert_eq!(settings.endpoint, "https://api-free.deepl.com/v2/translate");
    }

    #[test]
    fn rejects_non_http_translation_endpoint() {
        let error = StoredTranslationSettings {
            provider: OPENAI_COMPATIBLE_PROVIDER.to_owned(),
            endpoint: "file:///private/config.json".to_owned(),
            model: "test-model".to_owned(),
            system_prompt: "translate".to_owned(),
            timeout_ms: 8_000,
        }
        .normalized()
        .unwrap_err();

        assert_eq!(error, "translation endpoint must use HTTP or HTTPS");
    }

    #[test]
    fn stored_settings_serialization_has_no_api_key_field() {
        let serialized =
            serde_json::to_string(&StoredTranslationSettings::defaults_from_environment()).unwrap();

        assert!(!serialized.contains("apiKey"));
        assert!(!serialized.contains("api_key"));
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
            submitted_wall_ms: 0,
        };

        worker.submit(job(1));
        worker.submit(job(2));

        let state = queue.state.lock().unwrap();
        assert_eq!(state.latest.as_ref().unwrap().segment_id, 2);
    }
}
