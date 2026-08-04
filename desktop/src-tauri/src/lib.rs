use live_translator_core::{StablePrefixCoordinator, TokenPiece};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Manager;

pub mod asr;
pub mod audio;
pub mod local_mt;
pub mod session;
pub mod speaker;
pub mod translation;

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

    // Packaged builds look for a `models/` directory next to the executable,
    // so the app stays small and models are kept external.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let adjacent = parent.join("models");
            if adjacent.is_dir() {
                return adjacent;
            }
        }
    }

    // Fall back to the application data directory.
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| project_path("models"))
        .join("models")
}

fn is_file(path: impl AsRef<Path>) -> bool {
    path.as_ref().is_file()
}

/// Appends a line to a diagnostics log under the application data directory so
/// packaged builds can surface recognition/translation activity without a
/// terminal. Best-effort; failures are ignored.
pub fn log_line(line: &str) {
    use std::io::Write;
    let path = match std::env::var_os("LIVE_TRANSLATOR_LOG") {
        Some(path) => PathBuf::from(path),
        None => {
            let dir = std::env::var_os("LIVE_TRANSLATOR_MODEL_DIR")
                .map(PathBuf::from)
                .or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)));
            let Some(dir) = dir else { return };
            dir.join("diagnostics.log")
        }
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

#[tauri::command]
fn runtime_status(app: tauri::AppHandle) -> RuntimeStatus {
    let models = model_directory(&app);
    let asr_ready = is_file(models.join("ggml-small.bin"))
        || is_file(models.join("ggml-large-v3-turbo-q5_0.bin"));
    RuntimeStatus {
        platform: std::env::consts::OS,
        model_ready: asr_ready,
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
        .manage(session::SessionController::default())
        .invoke_handler(tauri::generate_handler![
            runtime_status,
            demo_subtitle,
            asr::settings::get_asr_settings,
            asr::settings::set_asr_model,
            translation::get_translation_settings,
            translation::save_translation_settings,
            session::list_microphone_devices,
            session::list_system_audio_devices,
            session::start_media_session,
            session::start_microphone_session,
            session::start_system_audio_session,
            session::start_browser_session,
            session::start_test_session,
            session::stop_session
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tauri application")
        .run(|app, event| {
            // Before the app exits, stop the recognition session so the
            // whisper Metal context is released cleanly. Otherwise whisper.cpp
            // aborts during its global Metal teardown (ggml_metal_rsets_free
            // asserts on non-empty residency sets).
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let sessions = app.state::<session::SessionController>();
                let _ = sessions.stop();
            }
        });
}
