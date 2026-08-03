//! Verifies the browser-tab WebSocket source path end to end: starts the
//! localhost listener, waits for a client, and reports captured 16 kHz mono
//! frames. Use together with the bundled `browser-tab-capture` extension, or
//! with `scripts/websocket_client_sim.py` to simulate a client.

use live_translator_core::WHISPER_SAMPLE_RATE;
use live_translator_desktop_lib::audio::websocket::BrowserTabPcmSource;
use live_translator_desktop_lib::audio::AudioFrameSource;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let seconds = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(5);

    let mut source = BrowserTabPcmSource::open()?;
    println!(
        "listening on ws://{}; connect the browser-tab extension or the simulator",
        live_translator_desktop_lib::audio::websocket::BROWSER_TAB_WEBSOCKET_ADDR
    );
    let stop = Arc::new(AtomicBool::new(false));
    let deadline_stop = Arc::clone(&stop);
    let _deadline_thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(seconds));
        deadline_stop.store(true, Ordering::Release);
    });

    let mut samples = 0_usize;
    let mut peak = 0_f32;
    let started = Instant::now();

    while !stop.load(Ordering::Acquire) {
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
        "wall_ms={} captured_ms={} peak={peak:.4} dropped_native_samples={}",
        started.elapsed().as_millis(),
        samples as u64 * 1_000 / u64::from(WHISPER_SAMPLE_RATE),
        source.dropped_samples()
    );
    Ok(())
}
