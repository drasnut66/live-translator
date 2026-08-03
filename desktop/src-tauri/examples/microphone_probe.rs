use live_translator_core::WHISPER_SAMPLE_RATE;
use live_translator_desktop_lib::audio::microphone::{
    available_input_devices, MicrophonePcmSource,
};
use live_translator_desktop_lib::audio::AudioFrameSource;
use std::error::Error;
use std::sync::atomic::AtomicBool;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let seconds = args
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3);
    let device_id = args.next();

    for device in available_input_devices()? {
        println!(
            "{}{} ({})",
            device.name,
            if device.is_default { " [default]" } else { "" },
            device.id
        );
    }

    let mut source = MicrophonePcmSource::open(device_id.as_deref())?;
    let stop = AtomicBool::new(false);
    let target_samples = seconds * WHISPER_SAMPLE_RATE as usize;
    let mut samples = 0_usize;
    let mut peak = 0_f32;

    while samples < target_samples {
        let Some(frame) = source.next_frame(&stop)? else {
            break;
        };
        samples += frame.samples.len();
        peak = frame
            .samples
            .iter()
            .fold(peak, |current, sample| current.max(sample.abs()));
    }

    source.stop()?;
    println!(
        "captured_ms={} peak={peak:.4} dropped_native_samples={}",
        samples as u64 * 1_000 / u64::from(WHISPER_SAMPLE_RATE),
        source.dropped_samples()
    );
    Ok(())
}
