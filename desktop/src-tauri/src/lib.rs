use live_translator_core::{StablePrefixCoordinator, TokenPiece};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Manager;

pub mod asr;
pub mod audio;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeStatus {
    platform: &'static str,
    model_ready: bool,
    vad_ready: bool,
    whisper_ready: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleSnapshot {
    stable_ja: String,
    unstable_ja: String,
}

fn project_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn configured_directory(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn model_directory(app: &tauri::AppHandle) -> PathBuf {
    if let Some(path) = configured_directory("LIVE_TRANSLATOR_MODEL_DIR") {
        return path;
    }

    if cfg!(debug_assertions) {
        return project_path("models");
    }

    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| project_path("models"))
        .join("models")
}

fn is_file(path: impl AsRef<Path>) -> bool {
    path.as_ref().is_file()
}

#[tauri::command]
fn runtime_status(app: tauri::AppHandle) -> RuntimeStatus {
    let models = model_directory(&app);
    RuntimeStatus {
        platform: std::env::consts::OS,
        model_ready: is_file(models.join("ggml-large-v3-turbo-q5_0.bin")),
        vad_ready: is_file(models.join("ggml-silero-v6.2.0.bin")),
        whisper_ready: true,
    }
}

#[tauri::command]
fn demo_subtitle() -> SubtitleSnapshot {
    let stable = vec![
        TokenPiece {
            id: 1,
            text: "パスタって".to_owned(),
        },
        TokenPiece {
            id: 2,
            text: "いろんな味にできるし、".to_owned(),
        },
    ];
    let mut current = stable.clone();
    current.push(TokenPiece {
        id: 3,
        text: "よくない？".to_owned(),
    });

    let mut coordinator = StablePrefixCoordinator::new(1, 0);
    coordinator.observe(&stable, 600);
    let update = coordinator.observe(&current, 900);

    SubtitleSnapshot {
        stable_ja: update.partial.stable_text_ja,
        unstable_ja: update.partial.unstable_text_ja,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![runtime_status, demo_subtitle])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
