use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const ASR_SETTINGS_FILE: &str = "asr-settings.json";
const DEFAULT_MODEL_FILE: &str = "ggml-small.bin";

pub const AVAILABLE_MODELS: [&str; 4] = [
    "ggml-small.bin",
    "ggml-large-v3-turbo-q5_0.bin",
    "ggml-large-v3-turbo-q8_0.bin",
    "ggml-base.bin",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrSettings {
    pub model: Option<String>,
}

impl Default for AsrSettings {
    fn default() -> Self {
        Self { model: None }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrSettingsPayload {
    pub model: Option<String>,
    pub available_models: Vec<&'static str>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(ASR_SETTINGS_FILE))
        .map_err(|_| "unable to locate the application settings directory".to_owned())
}

fn load_settings(app: &AppHandle) -> AsrSettings {
    let Ok(path) = settings_path(app) else {
        return AsrSettings::default();
    };
    match fs::read(&path) {
        Ok(contents) => serde_json::from_slice::<AsrSettings>(&contents).unwrap_or_default(),
        Err(error) if error.kind() == ErrorKind::NotFound => AsrSettings::default(),
        Err(_) => AsrSettings::default(),
    }
}

/// Returns the configured model file name: the persisted choice if present,
/// else the `LIVE_TRANSLATOR_WHISPER_MODEL` environment variable, else the
/// default small model.
pub fn selected_model_file(app: &AppHandle) -> String {
    if let Some(model) = load_settings(app).model {
        if !model.trim().is_empty() && AVAILABLE_MODELS.contains(&model.trim()) {
            return model.trim().to_owned();
        }
    }
    if let Ok(name) = std::env::var("LIVE_TRANSLATOR_WHISPER_MODEL") {
        let name = name.trim().to_owned();
        if !name.is_empty() {
            return name;
        }
    }
    DEFAULT_MODEL_FILE.to_owned()
}

#[tauri::command]
pub fn get_asr_settings(app: AppHandle) -> AsrSettingsPayload {
    AsrSettingsPayload {
        model: load_settings(&app).model,
        available_models: AVAILABLE_MODELS.to_vec(),
    }
}

#[tauri::command]
pub fn set_asr_model(app: AppHandle, model: String) -> Result<AsrSettingsPayload, String> {
    let model = model.trim().to_owned();
    if model.is_empty() {
        return Err("model must not be empty".to_owned());
    }
    if !AVAILABLE_MODELS.contains(&model.as_str()) {
        return Err(format!("unknown model: {model}"));
    }
    let path = settings_path(&app)?;
    let directory = path
        .parent()
        .ok_or_else(|| "unable to locate the application settings directory".to_owned())?;
    fs::create_dir_all(directory)
        .map_err(|_| "unable to create the application settings directory".to_owned())?;
    let settings = AsrSettings {
        model: Some(model.clone()),
    };
    let contents = serde_json::to_vec_pretty(&settings)
        .map_err(|_| "unable to encode ASR settings".to_owned())?;
    fs::write(&path, contents).map_err(|_| "unable to save ASR settings".to_owned())?;
    Ok(AsrSettingsPayload {
        model: Some(model),
        available_models: AVAILABLE_MODELS.to_vec(),
    })
}
