//! CLI probe for the first-run model downloader.
//! Usage: models_download_probe <dir> <vad|small|turbo|mt>
//! Downloads the chosen model into <dir> with progress + SHA-256 verification.
use live_translator_desktop_lib::models::{
    download_spec, ASR_MODEL_SMALL, ASR_MODEL_TURBO, LOCAL_MT_MODEL, VAD_MODEL,
};
use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let dir = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: models_download_probe <dir> <vad|small|turbo|mt>")?;
    let which = arguments
        .next()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "vad".to_owned());
    let spec = match which.as_str() {
        "vad" => &VAD_MODEL,
        "small" => &ASR_MODEL_SMALL,
        "turbo" => &ASR_MODEL_TURBO,
        "mt" => &LOCAL_MT_MODEL,
        other => return Err(format!("unknown model: {other}").into()),
    };
    let started = std::time::Instant::now();
    download_spec(&dir, spec, |spec, received, total, phase| {
        let percent = total
            .filter(|t| *t > 0)
            .map(|t| (received as f64 * 100.0 / t as f64) as u64)
            .unwrap_or(0);
        println!(
            "[{phase}] {} {}/{:?} {}%",
            spec.file, received, total, percent
        );
    })?;
    println!(
        "OK {} in {:.1}s",
        spec.file,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
