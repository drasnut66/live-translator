//! Long-run soak probe: replays the managed-session loop (FFmpeg source,
//! rolling Silero VAD, whisper.cpp decode, stable-prefix coordinator) for a
//! configurable number of seconds and reports memory growth, latency, and
//! queue health. Usage:
//!
//! ```text
//! cargo run --example soak_probe -- \
//!   <model.bin> <vad.bin> <media> [seconds]
//! ```
//!
//! Defaults to 600 seconds (10 minutes) of input media. The media is decoded
//! as fast as FFmpeg can deliver it, so wall time is much shorter than the
//! input duration; this stresses the pipeline without a 10-minute wait.
//!
//! Unlike the Tauri session this probe never calls a translation API. It
//! measures the ASR half of the pipeline end to end.

use live_translator_core::{StablePrefixCoordinator, TokenPiece, WHISPER_SAMPLE_RATE};
use live_translator_desktop_lib::asr::whisper::{WhisperEngine, WhisperEngineConfig};
use live_translator_desktop_lib::audio::ffmpeg::{FfmpegPcmSource, FfmpegSourceConfig};
use live_translator_desktop_lib::audio::vad::SileroVad;
use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

const MINIMUM_WINDOW_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 3 / 2;
const STEP_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize;
const MAXIMUM_WINDOW_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 15;
const VAD_INITIAL_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize / 2;
const VAD_STEP_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize / 4;
const RSS_POLL_SECONDS: u64 = 15;

struct Stats {
    segments: u64,
    partials: u64,
    revisions_this_segment: u64,
    revisions_total: u64,
    revisions_max: u64,
    first_partial_input_ms: Option<u64>,
    stable_commit_input_ms: Option<u64>,
    decode_total_ms: u64,
    decode_calls: u64,
    memory_samples: Vec<i64>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let model = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: soak_probe <model.bin> <vad.bin> <media> [seconds]")?;
    let vad_model = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: soak_probe <model.bin> <vad.bin> <media> [seconds]")?;
    let media = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: soak_probe <model.bin> <vad.bin> <media> [seconds]")?;
    let seconds = arguments
        .next()
        .and_then(|value| value.to_string_lossy().parse::<usize>().ok())
        .unwrap_or(600);

    let mut source_config = FfmpegSourceConfig::new(media.to_string_lossy());
    source_config.realtime = true;
    let mut source = FfmpegPcmSource::spawn(source_config)?;
    let mut engine = WhisperEngine::load(model, WhisperEngineConfig::default())?;
    let mut vad = SileroVad::load(vad_model)?;
    let session_started = Instant::now();
    let mut stats = Stats {
        segments: 0,
        partials: 0,
        revisions_this_segment: 0,
        revisions_total: 0,
        revisions_max: 0,
        first_partial_input_ms: None,
        stable_commit_input_ms: None,
        decode_total_ms: 0,
        decode_calls: 0,
        memory_samples: Vec::new(),
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
    let mut next_memory_poll = Instant::now();

    while total_samples < maximum_samples {
        if next_memory_poll.elapsed().as_secs() >= RSS_POLL_SECONDS {
            if let Some(rss) = resident_memory() {
                stats.memory_samples.push(rss);
            }
            next_memory_poll = Instant::now();
        }

        let frame = match source.next_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(error) => return Err(error.into()),
        };

        if audio.len() + frame.samples.len() > MAXIMUM_WINDOW_SAMPLES {
            if speech_seen && audio.len() >= MINIMUM_WINDOW_SAMPLES {
                latest_hypothesis = decode_and_observe(
                    &mut engine,
                    &mut coordinator,
                    &audio,
                    total_samples,
                    &mut stats,
                )?;
                last_decode_samples = total_samples;
            }
            finish_segment(&mut coordinator, &latest_hypothesis, total_samples, &mut stats);
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
                    latest_hypothesis = decode_and_observe(
                        &mut engine,
                        &mut coordinator,
                        &audio,
                        total_samples,
                        &mut stats,
                    )?;
                }
                finish_segment(&mut coordinator, &latest_hypothesis, total_samples, &mut stats);
                segment_id += 1;
                coordinator = StablePrefixCoordinator::new(segment_id, samples_to_ms(total_samples));
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
            latest_hypothesis = decode_and_observe(
                &mut engine,
                &mut coordinator,
                &audio,
                total_samples,
                &mut stats,
            )?;
            last_decode_samples = total_samples;
            while next_decode <= total_samples {
                next_decode += STEP_SAMPLES;
            }
        }
    }

    if speech_seen && audio.len() >= MINIMUM_WINDOW_SAMPLES && total_samples > last_decode_samples {
        latest_hypothesis = decode_and_observe(
            &mut engine,
            &mut coordinator,
            &audio,
            total_samples,
            &mut stats,
        )?;
    }
    finish_segment(&mut coordinator, &latest_hypothesis, total_samples, &mut stats);
    let _ = source.stop();

    report(&stats, total_samples, session_started);
    Ok(())
}

fn decode_and_observe(
    engine: &mut WhisperEngine,
    coordinator: &mut StablePrefixCoordinator,
    audio: &[f32],
    total_samples: usize,
    stats: &mut Stats,
) -> Result<Vec<TokenPiece>, String> {
    let started = Instant::now();
    let hypothesis = engine
        .transcribe(audio)
        .map_err(|error| error.to_string())?;
    let decode_ms = started.elapsed().as_millis() as u64;
    stats.decode_total_ms += decode_ms;
    stats.decode_calls += 1;
    stats.partials += 1;
    stats.revisions_this_segment += 1;
    stats.revisions_total += 1;
    stats.revisions_max = stats.revisions_max.max(stats.revisions_this_segment);

    let input_ms = samples_to_ms(total_samples);
    let update = coordinator.observe(&hypothesis, input_ms);
    if stats.first_partial_input_ms.is_none() {
        stats.first_partial_input_ms = Some(input_ms);
    }
    if update.commit.is_some() {
        stats.stable_commit_input_ms = Some(input_ms);
    }
    Ok(hypothesis)
}

fn finish_segment(
    coordinator: &mut StablePrefixCoordinator,
    latest_hypothesis: &[TokenPiece],
    total_samples: usize,
    stats: &mut Stats,
) {
    let input_ms = samples_to_ms(total_samples);
    if coordinator.finish(latest_hypothesis, input_ms).is_some() {
        stats.segments += 1;
        if stats.stable_commit_input_ms.is_none() {
            stats.stable_commit_input_ms = Some(input_ms);
        }
    }
    stats.revisions_this_segment = 0;
}

fn samples_to_ms(samples: usize) -> u64 {
    samples as u64 * 1_000 / u64::from(WHISPER_SAMPLE_RATE)
}

fn resident_memory() -> Option<i64> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("/bin/ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8(output.stdout).ok()?;
        text.trim().parse::<i64>().ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn report(stats: &Stats, total_samples: usize, session_started: Instant) {
    let input_ms = samples_to_ms(total_samples);
    let wall_ms = session_started.elapsed().as_millis() as u64;
    let average_decode = if stats.decode_calls > 0 {
        stats.decode_total_ms as f64 / stats.decode_calls as f64
    } else {
        0.0
    };
    let realtime_factor = if wall_ms > 0 {
        input_ms as f64 / wall_ms as f64
    } else {
        0.0
    };
    println!("=== soak probe summary ===");
    println!("input_ms={input_ms} wall_ms={wall_ms} realtime_factor={realtime_factor:.2}x");
    println!(
        "segments={} partials={} decode_calls={}",
        stats.segments, stats.partials, stats.decode_calls
    );
    println!(
        "revisions_total={} revisions_per_segment_max={}",
        stats.revisions_total, stats.revisions_max
    );
    println!("avg_decode_ms={average_decode:.1}");
    println!(
        "first_partial_input_ms={:?} stable_commit_input_ms={:?}",
        stats.first_partial_input_ms, stats.stable_commit_input_ms
    );
    if stats.memory_samples.len() >= 2 {
        let first = stats.memory_samples[0];
        let last = *stats.memory_samples.last().unwrap_or(&0);
        let peak = stats.memory_samples.iter().max().copied().unwrap_or(0);
        println!(
            "memory_samples={} first_rss_kb={first} last_rss_kb={last} peak_rss_kb={peak} delta_kb={}",
            stats.memory_samples.len(),
            last - first
        );
    }
}
