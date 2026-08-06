//! A/B quality probe: replays the managed-session loop (FFmpeg source, rolling
//! Silero VAD, whisper.cpp decode, stable-prefix coordinator) for a configurable
//! number of seconds and prints every committed subtitle plus decode statistics.
//!
//! Usage:
//!   asr_ab_probe <model.bin> <vad.bin> <media> [seconds]
//!
//! Optional env:
//!   LIVE_TRANSLATOR_DECODE_WINDOW_SECONDS=n   decode window (default 4)
use live_translator_core::{StablePrefixCoordinator, TokenPiece, WHISPER_SAMPLE_RATE};
use live_translator_desktop_lib::asr::whisper::{WhisperEngine, WhisperEngineConfig};
use live_translator_desktop_lib::audio::ffmpeg::{FfmpegPcmSource, FfmpegSourceConfig};
use live_translator_desktop_lib::audio::vad::SileroVad;
use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

const MINIMUM_WINDOW_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 3 / 2;
const STEP_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize;
const MAXIMUM_WINDOW_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 10;
const VAD_INITIAL_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize / 2;
const VAD_STEP_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize / 4;

struct Stats {
    decode_total_ms: u64,
    decode_calls: u64,
    decode_max_ms: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let model = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: asr_ab_probe <model.bin> <vad.bin> <media> [seconds]")?;
    let vad_model = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: asr_ab_probe <model.bin> <vad.bin> <media> [seconds]")?;
    let media = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: asr_ab_probe <model.bin> <vad.bin> <media> [seconds]")?;
    let seconds = arguments
        .next()
        .and_then(|value| value.to_string_lossy().parse::<usize>().ok())
        .unwrap_or(180);

    let window_seconds = std::env::var("LIVE_TRANSLATOR_DECODE_WINDOW_SECONDS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(4)
        .max(2)
        .min(10);
    let decode_window_samples = WHISPER_SAMPLE_RATE as usize * window_seconds;

    let mut source_config = FfmpegSourceConfig::new(media.to_string_lossy());
    source_config.realtime = true;
    let mut source = FfmpegPcmSource::spawn(source_config)?;
    let mut engine = WhisperEngine::load(&model, WhisperEngineConfig::default())?;
    let mut vad = SileroVad::load(vad_model)?;
    let session_started = Instant::now();
    let mut stats = Stats {
        decode_total_ms: 0,
        decode_calls: 0,
        decode_max_ms: 0,
    };

    let mut segment_id = 1_u64;
    let mut coordinator = StablePrefixCoordinator::new(segment_id, 0);
    let mut audio = Vec::with_capacity(MAXIMUM_WINDOW_SAMPLES);
    let mut latest_hypothesis: Vec<TokenPiece> = Vec::new();
    let mut total_samples = 0_usize;
    let mut next_decode = MINIMUM_WINDOW_SAMPLES;
    let mut next_vad = VAD_INITIAL_SAMPLES;
    let mut last_decode_samples = 0_usize;
    let mut speech_seen = false;
    let maximum_samples = seconds * WHISPER_SAMPLE_RATE as usize;

    while total_samples < maximum_samples {
        let frame = match source.next_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(error) => return Err(error.into()),
        };

        if audio.len() + frame.samples.len() > MAXIMUM_WINDOW_SAMPLES {
            if speech_seen && audio.len() >= MINIMUM_WINDOW_SAMPLES {
                latest_hypothesis = decode_and_emit(
                    &mut engine,
                    &mut coordinator,
                    &audio,
                    total_samples,
                    decode_window_samples,
                    &mut stats,
                )?;
                last_decode_samples = total_samples;
            }
            finish_segment(&mut coordinator, &latest_hypothesis, total_samples);
            segment_id += 1;
            coordinator = StablePrefixCoordinator::new(segment_id, samples_to_ms(total_samples));
            audio.clear();
            latest_hypothesis.clear();
            vad.reset();
            speech_seen = false;
            next_decode = total_samples + MINIMUM_WINDOW_SAMPLES;
            next_vad = total_samples + VAD_INITIAL_SAMPLES;
        }

        total_samples += frame.samples.len();
        audio.extend(frame.samples);

        if total_samples >= next_vad {
            let observation = vad.observe(&audio)?;
            speech_seen = observation.speech_seen;
            while next_vad <= total_samples {
                next_vad += VAD_STEP_SAMPLES;
            }
            if observation.end_of_speech {
                if audio.len() >= MINIMUM_WINDOW_SAMPLES && total_samples > last_decode_samples {
                    latest_hypothesis = decode_and_emit(
                        &mut engine,
                        &mut coordinator,
                        &audio,
                        total_samples,
                        decode_window_samples,
                        &mut stats,
                    )?;
                }
                finish_segment(&mut coordinator, &latest_hypothesis, total_samples);
                segment_id += 1;
                coordinator =
                    StablePrefixCoordinator::new(segment_id, samples_to_ms(total_samples));
                audio.clear();
                latest_hypothesis.clear();
                vad.reset();
                speech_seen = false;
                next_decode = total_samples + MINIMUM_WINDOW_SAMPLES;
                next_vad = total_samples + VAD_INITIAL_SAMPLES;
                last_decode_samples = total_samples;
                continue;
            }
        }

        if speech_seen && total_samples >= next_decode {
            latest_hypothesis = decode_and_emit(
                &mut engine,
                &mut coordinator,
                &audio,
                total_samples,
                decode_window_samples,
                &mut stats,
            )?;
            last_decode_samples = total_samples;
            while next_decode <= total_samples {
                next_decode += STEP_SAMPLES;
            }
        }
    }

    if speech_seen && audio.len() >= MINIMUM_WINDOW_SAMPLES && total_samples > last_decode_samples {
        latest_hypothesis = decode_and_emit(
            &mut engine,
            &mut coordinator,
            &audio,
            total_samples,
            decode_window_samples,
            &mut stats,
        )?;
    }
    finish_segment(&mut coordinator, &latest_hypothesis, total_samples);
    let _ = source.stop();

    let input_ms = samples_to_ms(total_samples);
    let wall_ms = session_started.elapsed().as_millis() as u64;
    let avg = if stats.decode_calls > 0 {
        stats.decode_total_ms / stats.decode_calls
    } else {
        0
    };
    println!(
        "[stats] model={} window={window_seconds}s input={input_ms}ms wall={wall_ms}ms realtime={:.2}x decodes={} avg_decode={}ms max_decode={}ms",
        model.display(),
        wall_ms as f64 / input_ms as f64,
        stats.decode_calls,
        avg,
        stats.decode_max_ms
    );
    Ok(())
}

fn decode_and_emit(
    engine: &mut WhisperEngine,
    coordinator: &mut StablePrefixCoordinator,
    audio: &[f32],
    total_samples: usize,
    decode_window_samples: usize,
    stats: &mut Stats,
) -> Result<Vec<TokenPiece>, String> {
    let started = Instant::now();
    let decode_start = audio.len().saturating_sub(decode_window_samples);
    let window = &audio[decode_start..];
    let hypothesis = engine
        .transcribe(window)
        .map_err(|error| error.to_string())?;
    let decode_ms = started.elapsed().as_millis() as u64;
    stats.decode_total_ms += decode_ms;
    stats.decode_calls += 1;
    stats.decode_max_ms = stats.decode_max_ms.max(decode_ms);

    let input_ms = samples_to_ms(total_samples);
    let update = coordinator.observe(&hypothesis, input_ms);
    if let Some(commit) = update.commit {
        println!(
            "[commit] seg={} {}-{}ms JA=\"{}\" skip={}",
            commit.segment_id,
            commit.start_ms,
            commit.end_ms,
            commit.text_ja,
            commit.translation_skippable
        );
    }
    Ok(hypothesis)
}

fn finish_segment(
    coordinator: &mut StablePrefixCoordinator,
    latest_hypothesis: &[TokenPiece],
    total_samples: usize,
) {
    let input_ms = samples_to_ms(total_samples);
    if let Some(commit) = coordinator.finish(latest_hypothesis, input_ms) {
        println!(
            "[commit] seg={} {}-{}ms JA=\"{}\" skip={}",
            commit.segment_id,
            commit.start_ms,
            commit.end_ms,
            commit.text_ja,
            commit.translation_skippable
        );
    }
}

fn samples_to_ms(samples: usize) -> u64 {
    samples as u64 * 1_000 / u64::from(WHISPER_SAMPLE_RATE)
}
