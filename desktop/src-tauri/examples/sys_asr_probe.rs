//! System-audio ASR probe: captures system audio and runs the same rolling
//! decode the session uses, decoding only the most recent few seconds so a
//! multi-speaker window does not mix voices. Built once and kept at a fixed
//! path so the Screen Recording grant stays valid across runs.
use live_translator_core::{StablePrefixCoordinator, WHISPER_SAMPLE_RATE};
use live_translator_desktop_lib::asr::whisper::{WhisperEngine, WhisperEngineConfig};
use live_translator_desktop_lib::audio::system_audio::SystemAudioPcmSource;
use live_translator_desktop_lib::audio::AudioFrameSource;
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MINIMUM_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 3 / 2;
const STEP_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize;
const WINDOW_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 12;
const DECODE_WINDOW_SECS: usize = 4;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut arguments = std::env::args_os().skip(1);
    let model = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: sys_asr_probe <model.bin> [seconds]")?;
    let seconds = arguments
        .next()
        .and_then(|value| value.to_string_lossy().parse::<u64>().ok())
        .unwrap_or(60);

    let stop = Arc::new(AtomicBool::new(false));
    let deadline_stop = Arc::clone(&stop);
    let _deadline = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(seconds));
        deadline_stop.store(true, Ordering::Release);
    });

    let mut source = SystemAudioPcmSource::open(None)?;
    let mut engine = WhisperEngine::load(&model, WhisperEngineConfig::default())?;
    let mut coordinator = StablePrefixCoordinator::new(1, 0);
    let mut audio: Vec<f32> = Vec::with_capacity(WINDOW_SAMPLES);
    let mut total_samples = 0_usize;
    let mut next_decode = MINIMUM_SAMPLES;
    let mut latest: Vec<live_translator_core::TokenPiece> = Vec::new();
    let started = Instant::now();
    let mut last_print = Instant::now();

    println!("sys ASR running for {seconds}s...");
    while !stop.load(Ordering::Acquire) {
        let Some(frame) = source.next_frame(&stop)? else {
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
        let decode_start = audio
            .len()
            .saturating_sub(WHISPER_SAMPLE_RATE as usize * DECODE_WINDOW_SECS);
        let window = &audio[decode_start..];
        latest = engine.transcribe(window).map_err(|e| e.to_string())?;
        let elapsed_ms = total_samples as u64 * 1000 / u64::from(WHISPER_SAMPLE_RATE);
        let update = coordinator.observe(&latest, elapsed_ms);
        let text = format!(
            "{}{}",
            update.partial.stable_text_ja, update.partial.unstable_text_ja
        );
        if !text.trim().is_empty() && last_print.elapsed().as_secs_f32() > 1.5 {
            println!(
                "[{:>5} ms wall={:>5} ms] {}",
                elapsed_ms,
                started.elapsed().as_millis(),
                text
            );
            last_print = Instant::now();
        }
        next_decode += STEP_SAMPLES;
    }
    let _ = source.stop();
    println!("done");
    Ok(())
}
