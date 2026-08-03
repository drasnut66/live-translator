use live_translator_core::{StablePrefixCoordinator, WHISPER_SAMPLE_RATE};
use live_translator_desktop_lib::asr::whisper::{WhisperEngine, WhisperEngineConfig};
use live_translator_desktop_lib::audio::ffmpeg::{FfmpegPcmSource, FfmpegSourceConfig};
use std::error::Error;
use std::path::PathBuf;

const MINIMUM_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 3 / 2;
const STEP_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize;
const WINDOW_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 8;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let model = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: live_asr_probe <model.bin> <media> [seconds]")?;
    let media = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: live_asr_probe <model.bin> <media> [seconds]")?;
    let seconds = arguments
        .next()
        .and_then(|value| value.to_string_lossy().parse::<usize>().ok())
        .unwrap_or(12);

    let mut source_config = FfmpegSourceConfig::new(media.to_string_lossy());
    source_config.realtime = false;
    let mut source = FfmpegPcmSource::spawn(source_config)?;
    let mut engine = WhisperEngine::load(model, WhisperEngineConfig::default())?;
    let mut coordinator = StablePrefixCoordinator::new(1, 0);
    let mut audio = Vec::with_capacity(WINDOW_SAMPLES);
    let mut total_samples = 0_usize;
    let mut next_decode = MINIMUM_SAMPLES;
    let maximum_samples = seconds * WHISPER_SAMPLE_RATE as usize;
    let mut latest = Vec::new();

    while total_samples < maximum_samples {
        let Some(frame) = source.next_frame()? else {
            break;
        };
        total_samples += frame.samples.len();
        audio.extend(frame.samples);
        if audio.len() > WINDOW_SAMPLES {
            audio.drain(..audio.len() - WINDOW_SAMPLES);
        }

        if total_samples < next_decode {
            continue;
        }
        latest = engine.transcribe(&audio)?;
        let elapsed_ms = total_samples as u64 * 1000 / u64::from(WHISPER_SAMPLE_RATE);
        let update = coordinator.observe(&latest, elapsed_ms);
        println!(
            "[{elapsed_ms:>5} ms] {}{}",
            update.partial.stable_text_ja, update.partial.unstable_text_ja
        );
        next_decode += STEP_SAMPLES;
    }

    let elapsed_ms = total_samples as u64 * 1000 / u64::from(WHISPER_SAMPLE_RATE);
    if let Some(commit) = coordinator.finish(&latest, elapsed_ms) {
        println!("[final] {}", commit.text_ja);
    }
    source.stop()?;
    Ok(())
}
