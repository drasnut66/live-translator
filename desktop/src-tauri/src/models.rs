//! First-run model downloader with progress events and SHA-256 verification.
//! Used by the "lite" portable package: the app downloads the required Whisper /
//! VAD / Hy-MT files into the model directory on demand, instead of shipping
//! them in the installer.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

pub const MODEL_PROGRESS_EVENT: &str = "models:progress";
pub const MODEL_FINISHED_EVENT: &str = "models:finished";

pub struct ModelSpec {
    pub file: &'static str,
    pub group: &'static str,
    pub subdir: &'static str,
    pub url_path: &'static str,
    pub sha256: &'static str,
    pub size_hint: u64,
}

pub const VAD_MODEL: ModelSpec = ModelSpec {
    file: "ggml-silero-v6.2.0.bin",
    group: "vad",
    subdir: "",
    url_path: "ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin",
    sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987",
    size_hint: 800_000,
};

pub const ASR_MODEL_SMALL: ModelSpec = ModelSpec {
    file: "ggml-small.bin",
    group: "asr",
    subdir: "",
    url_path: "ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
    sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
    size_hint: 466_000_000,
};

pub const ASR_MODEL_TURBO: ModelSpec = ModelSpec {
    file: "ggml-large-v3-turbo-q5_0.bin",
    group: "asr",
    subdir: "",
    url_path: "ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    size_hint: 547_000_000,
};

pub const LOCAL_MT_MODEL: ModelSpec = ModelSpec {
    file: "HY-MT1.5-1.8B-Q4_K_M.gguf",
    group: "mt",
    subdir: "hymt",
    url_path: "tencent/HY-MT1.5-1.8B-GGUF/resolve/main/HY-MT1.5-1.8B-Q4_K_M.gguf",
    sha256: "4383ac0c3c8e476de98ff979c2a3f069f8c4fb385e7860cf2d28da896cc477c7",
    size_hint: 1_080_000_000,
};

pub const ASR_MODELS: &[ModelSpec] = &[ASR_MODEL_SMALL, ASR_MODEL_TURBO];

/// Default mirror is hf-mirror.com (fast in China). Override with
/// LIVE_TRANSLATOR_MODEL_MIRROR, e.g. https://huggingface.co
fn mirror_base() -> String {
    std::env::var("LIVE_TRANSLATOR_MODEL_MIRROR")
        .unwrap_or_else(|_| "https://hf-mirror.com".to_owned())
}

pub fn spec_path(base: &Path, spec: &ModelSpec) -> PathBuf {
    if spec.subdir.is_empty() {
        base.join(spec.file)
    } else {
        base.join(spec.subdir).join(spec.file)
    }
}

pub fn sha256_of(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

pub fn is_valid(base: &Path, spec: &ModelSpec) -> bool {
    let path = spec_path(base, spec);
    path.is_file() && sha256_of(&path).as_deref() == Some(spec.sha256)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatusPayload {
    pub file: String,
    pub group: &'static str,
    pub present: bool,
    pub valid: bool,
    pub size_hint: u64,
}

pub fn status(base: &Path) -> Vec<ModelStatusPayload> {
    let mut items = Vec::new();
    for spec in ASR_MODELS {
        items.push(payload(base, spec));
    }
    items.push(payload(base, &VAD_MODEL));
    items.push(payload(base, &LOCAL_MT_MODEL));
    items
}

fn payload(base: &Path, spec: &ModelSpec) -> ModelStatusPayload {
    let path = spec_path(base, spec);
    let present = path.is_file();
    let valid = present && sha256_of(&path).as_deref() == Some(spec.sha256);
    ModelStatusPayload {
        file: spec.file.to_owned(),
        group: spec.group,
        present,
        valid,
        size_hint: spec.size_hint,
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProgressPayload {
    pub file: String,
    pub received: u64,
    pub total: Option<u64>,
    pub phase: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFinishedPayload {
    pub success: bool,
    pub message: String,
}

/// Download one model file with periodic progress callbacks.
/// Skips the file when it already exists and passes the SHA-256 check.
pub fn download_spec(
    base: &Path,
    spec: &ModelSpec,
    mut emit: impl FnMut(&ModelSpec, u64, Option<u64>, &str),
) -> Result<(), String> {
    let path = spec_path(base, spec);
    if is_valid(base, spec) {
        emit(spec, 0, None, "already-valid");
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let url = format!("{}/{}", mirror_base(), spec.url_path);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| format!("http client: {error}"))?;
    let mut response = client
        .get(&url)
        .send()
        .map_err(|error| format!("GET {url}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {} for {url}", response.status()));
    }
    let total = response.content_length();
    let tmp = path.with_extension("part");
    let mut file = fs::File::create(&tmp)
        .map_err(|error| format!("create {}: {error}", tmp.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut received: u64 = 0;
    let mut last_emit = Instant::now();
    loop {
        let n = response
            .read(&mut buffer)
            .map_err(|error| format!("read: {error}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])
            .map_err(|error| format!("write: {error}"))?;
        hasher.update(&buffer[..n]);
        received += n as u64;
        if last_emit.elapsed().as_millis() >= 200 {
            emit(spec, received, total, "downloading");
            last_emit = Instant::now();
        }
    }
    file.flush().map_err(|error| format!("flush: {error}"))?;
    drop(file);
    let digest = format!("{:x}", hasher.finalize());
    if digest != spec.sha256 {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "checksum mismatch for {}: expected {}, got {digest}",
            spec.file, spec.sha256
        ));
    }
    fs::rename(&tmp, &path).map_err(|error| format!("rename: {error}"))?;
    emit(spec, received, total, "done");
    Ok(())
}

static ASR_DOWNLOADING: AtomicBool = AtomicBool::new(false);
static MT_DOWNLOADING: AtomicBool = AtomicBool::new(false);

fn emit_progress(app: &AppHandle, spec: &ModelSpec, received: u64, total: Option<u64>, phase: &str) {
    let _ = app.emit(
        MODEL_PROGRESS_EVENT,
        ModelProgressPayload {
            file: spec.file.to_owned(),
            received,
            total,
            phase: phase.to_owned(),
        },
    );
}

fn emit_finished(app: &AppHandle, success: bool, message: String) {
    let _ = app.emit(MODEL_FINISHED_EVENT, ModelFinishedPayload { success, message });
}

#[tauri::command]
pub fn models_status(app: AppHandle) -> Vec<ModelStatusPayload> {
    status(&crate::model_directory(&app))
}

#[tauri::command]
pub fn download_asr_models(app: AppHandle) -> Result<(), String> {
    if ASR_DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("模型下载已在后台进行".to_owned());
    }
    std::thread::spawn(move || {
        let dir = crate::model_directory(&app);
        let mut specs: Vec<&ModelSpec> = vec![&VAD_MODEL];
        specs.extend(ASR_MODELS.iter());
        let result = (|| {
            for spec in specs {
                download_spec(&dir, spec, |s, r, t, ph| emit_progress(&app, s, r, t, ph))?;
            }
            Ok::<(), String>(())
        })();
        ASR_DOWNLOADING.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => emit_finished(&app, true, "ASR 模型下载完成".to_owned()),
            Err(error) => emit_finished(&app, false, error),
        }
    });
    Ok(())
}

#[tauri::command]
pub fn download_local_mt_model(app: AppHandle) -> Result<(), String> {
    if MT_DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("本地翻译模型下载已在后台进行".to_owned());
    }
    std::thread::spawn(move || {
        let dir = crate::model_directory(&app);
        let result = download_spec(&dir, &LOCAL_MT_MODEL, |s, r, t, ph| {
            emit_progress(&app, s, r, t, ph)
        });
        MT_DOWNLOADING.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => emit_finished(&app, true, "本地翻译模型下载完成".to_owned()),
            Err(error) => emit_finished(&app, false, error),
        }
    });
    Ok(())
}
