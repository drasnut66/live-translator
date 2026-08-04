use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_PORT: u16 = 8090;
const HY_MT_PROMPT: &str =
    "Translate the following segment into Chinese, without additional explanation.\n\n";

pub const HY_MT_MODEL_FILE: &str = "hymt/HY-MT1.5-1.8B-Q4_K_M.gguf";
#[cfg(target_os = "windows")]
pub const LLAMA_SERVER_FILE: &str = "hymt/llama-server.exe";
#[cfg(not(target_os = "windows"))]
pub const LLAMA_SERVER_FILE: &str = "hymt/llama-server";

/// Offline Japanese-to-Chinese translation by running the Tencent Hy-MT1.5
/// GGUF model through a long-lived llama-server subprocess (CPU mode). The
/// server keeps the model resident, so each short segment translates in well
/// under a second. This sidesteps the old bundled llama.cpp (which cannot read
/// the hunyuan-dense architecture) and the Metal path (which ballooned memory).
pub struct LocalMtTranslator {
    child: Arc<MutexChild>,
    client: Client,
    endpoint: String,
    prompt: String,
}

// Child is not Sync; wrap in a small mutex holder.
struct MutexChild(std::sync::Mutex<Option<Child>>);

impl LocalMtTranslator {
    pub fn new(model_dir: impl AsRef<Path>) -> Result<Self, String> {
        let model_dir = model_dir.as_ref().to_path_buf();
        let model_path = model_dir.join(HY_MT_MODEL_FILE);
        if !model_path.is_file() {
            return Err(format!(
                "Hy-MT translation model is missing: {}",
                model_path.display()
            ));
        }
        let server_path = model_dir.join(LLAMA_SERVER_FILE);
        if !server_path.is_file() {
            return Err(format!(
                "llama-server binary is missing at {}",
                server_path.display()
            ));
        }

        let port = DEFAULT_PORT;
        let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");
        let child = spawn_server(&server_path, &model_path, port)
            .map_err(|error| format!("failed to start llama-server: {error}"))?;
        let child = Arc::new(MutexChild(std::sync::Mutex::new(Some(child))));

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| format!("failed to build local translation client: {error}"))?;

        Ok(Self {
            child,
            client,
            endpoint,
            prompt: HY_MT_PROMPT.to_owned(),
        })
    }

    pub fn translate(&self, text_ja: &str) -> Result<String, String> {
        let request = ChatCompletionRequest {
            model: "hymt",
            messages: vec![ChatMessage {
                role: "user",
                content: format!("{}{}", self.prompt, text_ja),
            }],
            max_tokens: 128,
            temperature: 0.7,
            top_k: Some(20),
            top_p: Some(0.6),
            repeat_penalty: Some(1.05),
        };
        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .map_err(|error| format!("local translation request failed: {error}"))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|error| format!("failed to read local translation response: {error}"))?;
        if !status.is_success() {
            return Err(format!("local translation returned {status}"));
        }
        let parsed: ChatCompletionResponse = serde_json::from_str(&body)
            .map_err(|error| format!("invalid local translation response: {error}"))?;
        let text = parsed
            .choices
            .first()
            .and_then(|choice| {
                choice
                    .message
                    .content
                    .trim()
                    .split('\n')
                    .next()
                    .map(str::to_owned)
            })
            .filter(|text| !text.is_empty())
            .ok_or_else(|| "local translation returned no text".to_owned())?;
        Ok(text)
    }
}

impl Drop for LocalMtTranslator {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.child.0.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

fn spawn_server(server_path: &Path, model_path: &Path, port: u16) -> Result<Child, String> {
    let mut command = Command::new(server_path);
    command
        .arg("-m")
        .arg(model_path)
        .arg("--port")
        .arg(port.to_string())
        .arg("-ngl")
        .arg("0")
        .arg("-t")
        .arg("2")
        .arg("--host")
        .arg("127.0.0.1")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // llama-server loads its bundled ggml/llama dylibs via @rpath; point the
    // dynamic loader at the sibling directory that carries them.
    #[cfg(target_os = "macos")]
    {
        if let Some(dir) = server_path.parent() {
            command.env("DYLD_LIBRARY_PATH", dir);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        if let Some(dir) = server_path.parent() {
            command.env(
                "PATH",
                format!(
                    "{};{}",
                    dir.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }
    }
    let child = command
        .spawn()
        .map_err(|error| format!("failed to spawn {server_path:?}: {error}"))?;

    // Wait for the server to load the model. /health returns HTTP 200 only
    // after the model is resident; during loading it returns 503.
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| format!("failed to build probe client: {error}"))?;
    let probe = format!("http://127.0.0.1:{port}/health");
    let mut ready = false;
    for _ in 0..120 {
        match client.get(&probe).send() {
            Ok(response) if response.status().is_success() => {
                ready = true;
                break;
            }
            _ => std::thread::sleep(Duration::from_millis(500)),
        }
    }
    if ready {
        Ok(child)
    } else {
        Err("llama-server did not become ready within 60s".to_owned())
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: &'static str,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
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
