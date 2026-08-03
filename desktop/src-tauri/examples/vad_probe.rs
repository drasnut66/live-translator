use live_translator_core::WHISPER_SAMPLE_RATE;
use live_translator_desktop_lib::audio::ffmpeg::{FfmpegPcmSource, FfmpegSourceConfig};
use live_translator_desktop_lib::audio::vad::SileroVad;
use std::error::Error;
use std::io;
use std::path::PathBuf;

const VAD_INITIAL_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize / 2;
const VAD_STEP_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize / 4;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let model = PathBuf::from(
        args.next()
            .ok_or("usage: vad_probe MODEL MEDIA [SECONDS]")?,
    );
    let media = PathBuf::from(
        args.next()
            .ok_or("usage: vad_probe MODEL MEDIA [SECONDS]")?,
    );
    let duration_seconds = args
        .next()
        .and_then(|value| value.to_string_lossy().parse::<u64>().ok())
        .unwrap_or(30);
    let sample_limit = duration_seconds as usize * WHISPER_SAMPLE_RATE as usize;

    let mut vad = SileroVad::load(model).map_err(io::Error::other)?;
    let mut source_config = FfmpegSourceConfig::new(media.to_string_lossy());
    source_config.realtime = false;
    let mut source = FfmpegPcmSource::spawn(source_config)?;
    let mut audio = Vec::new();
    let mut total_samples = 0_usize;
    let mut next_vad = VAD_INITIAL_SAMPLES;
    let mut endpoint_count = 0_u64;

    while total_samples < sample_limit {
        let Some(frame) = source.next_frame()? else {
            break;
        };
        total_samples += frame.samples.len();
        audio.extend(frame.samples);
        if total_samples < next_vad {
            continue;
        }

        let observation = vad.observe(&audio).map_err(io::Error::other)?;
        while next_vad <= total_samples {
            next_vad += VAD_STEP_SAMPLES;
        }
        if observation.end_of_speech {
            endpoint_count += 1;
            println!(
                "endpoint={endpoint_count} input_ms={} buffered_ms={} last_speech_end_ms={:?}",
                samples_to_milliseconds(total_samples),
                samples_to_milliseconds(audio.len()),
                observation.last_speech_end_ms,
            );
            audio.clear();
            vad.reset();
            next_vad = total_samples + VAD_INITIAL_SAMPLES;
        }
    }

    source.stop()?;
    println!(
        "completed input_ms={} endpoints={endpoint_count}",
        samples_to_milliseconds(total_samples)
    );
    Ok(())
}

fn samples_to_milliseconds(samples: usize) -> u64 {
    samples as u64 * 1_000 / u64::from(WHISPER_SAMPLE_RATE)
}
