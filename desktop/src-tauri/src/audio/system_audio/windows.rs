use crate::audio::microphone::{AudioDevice, BufferedPcmCapture, CaptureSampleSink};
use crate::audio::AudioFrameSource;
use live_translator_core::AudioFrame;
use std::io;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use wasapi::{
    deinitialize, initialize_mta, AudioClient, Device, DeviceEnumerator, Direction, SampleType,
    StreamMode, WasapiError, WaveFormat,
};

const SAMPLE_RATE: usize = 48_000;
const CHANNELS: usize = 2;
const BUFFER_DURATION_HNS: i64 = 200_000;
const EVENT_TIMEOUT_MS: u32 = 100;

pub fn available_system_audio_devices() -> Result<Vec<AudioDevice>, String> {
    initialize_mta()
        .ok()
        .map_err(|error| format!("failed to initialize Windows audio: {error}"))?;
    let result = enumerate_output_devices();
    deinitialize();
    result
}

pub struct SystemAudioPcmSource {
    capture: BufferedPcmCapture,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl SystemAudioPcmSource {
    pub fn open(device_id: Option<&str>) -> io::Result<Self> {
        let capture = BufferedPcmCapture::new(SAMPLE_RATE as u32)?;
        let sink = capture.sample_sink();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let selected_device = device_id.map(ToOwned::to_owned);
        let join = thread::Builder::new()
            .name("live-translator-wasapi-loopback".to_owned())
            .spawn(move || capture_loop(selected_device, thread_stop, sink))
            .map_err(|error| {
                io::Error::other(format!("failed to start WASAPI loopback: {error}"))
            })?;

        Ok(Self {
            capture,
            stop,
            join: Some(join),
        })
    }

    pub fn dropped_samples(&self) -> u64 {
        self.capture.dropped_samples()
    }

    fn close(&mut self) -> io::Result<()> {
        self.stop.store(true, Ordering::Release);
        self.capture.stop();
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| io::Error::other("WASAPI loopback thread panicked"))?;
        }
        Ok(())
    }
}

impl AudioFrameSource for SystemAudioPcmSource {
    fn next_frame(&mut self, stop: &AtomicBool) -> io::Result<Option<AudioFrame>> {
        self.capture.next_frame(stop)
    }

    fn stop(&mut self) -> io::Result<()> {
        self.close()
    }
}

impl Drop for SystemAudioPcmSource {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn enumerate_output_devices() -> Result<Vec<AudioDevice>, String> {
    let enumerator = DeviceEnumerator::new().map_err(|error| error.to_string())?;
    let default_id = enumerator
        .get_default_device(&Direction::Render)
        .and_then(|device| device.get_id())
        .map_err(|error| error.to_string())?;
    let devices = enumerator
        .get_device_collection(&Direction::Render)
        .map_err(|error| error.to_string())?;
    let mut result = Vec::new();
    for device in &devices {
        let device = device.map_err(|error| error.to_string())?;
        let id = device.get_id().map_err(|error| error.to_string())?;
        result.push(AudioDevice {
            name: device
                .get_friendlyname()
                .unwrap_or_else(|_| "Windows output device".to_owned()),
            is_default: id == default_id,
            id,
        });
    }
    result.sort_by_key(|device| !device.is_default);
    Ok(result)
}

fn capture_loop(device_id: Option<String>, stop: Arc<AtomicBool>, sink: CaptureSampleSink) {
    let result = run_capture_loop(device_id.as_deref(), &stop, &sink);
    if let Err(error) = result {
        sink.fail(format!("WASAPI loopback failed: {error}"));
    }
    sink.mark_stopped();
}

fn run_capture_loop(
    device_id: Option<&str>,
    stop: &AtomicBool,
    sink: &CaptureSampleSink,
) -> Result<(), String> {
    initialize_mta()
        .ok()
        .map_err(|error| format!("failed to initialize Windows audio: {error}"))?;
    let result = run_capture_loop_after_com(device_id, stop, sink);
    deinitialize();
    result
}

fn run_capture_loop_after_com(
    device_id: Option<&str>,
    stop: &AtomicBool,
    sink: &CaptureSampleSink,
) -> Result<(), String> {
    let enumerator = DeviceEnumerator::new().map_err(|error| error.to_string())?;
    let device = match device_id {
        Some(id) => enumerator.get_device(id),
        None => enumerator.get_default_device(&Direction::Render),
    }
    .map_err(|error| error.to_string())?;
    run_loopback(device, stop, sink)
}

fn run_loopback(device: Device, stop: &AtomicBool, sink: &CaptureSampleSink) -> Result<(), String> {
    let desired_format = WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE, CHANNELS, None);
    let mut client = device
        .get_iaudioclient()
        .map_err(|error| error.to_string())?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: BUFFER_DURATION_HNS,
    };
    client
        .initialize_client(&desired_format, &Direction::Capture, &mode)
        .map_err(|error| error.to_string())?;
    let event = client
        .set_get_eventhandle()
        .map_err(|error| error.to_string())?;
    let capture_client = client
        .get_audiocaptureclient()
        .map_err(|error| error.to_string())?;
    client.start_stream().map_err(|error| error.to_string())?;

    let result = loop {
        if stop.load(Ordering::Acquire) {
            break Ok(());
        }
        match event.wait_for_event(EVENT_TIMEOUT_MS) {
            Ok(()) => {}
            Err(WasapiError::EventTimeout) => continue,
            Err(error) => break Err(error.to_string()),
        }
        loop {
            let Some(packet_frames) = capture_client
                .get_next_packet_size()
                .map_err(|error| error.to_string())?
            else {
                break Err("WASAPI loopback did not report a packet size".to_owned());
            };
            if packet_frames == 0 {
                break;
            }
            let mut bytes = vec![0_u8; packet_frames as usize * CHANNELS * 4];
            let (frames, info) = capture_client
                .read_from_device(&mut bytes)
                .map_err(|error| error.to_string())?;
            if frames == 0 {
                break;
            }
            let mono = if info.flags.silent {
                vec![0.0; frames as usize]
            } else {
                decode_interleaved_stereo_f32(&bytes[..frames as usize * CHANNELS * 4])
            };
            sink.push_samples(mono);
        }
    };
    let stop_result = client.stop_stream().map_err(|error| error.to_string());
    result.and(stop_result)
}

fn decode_interleaved_stereo_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(CHANNELS * 4)
        .map(|frame| {
            let left = f32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]);
            let right = f32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);
            let left = if left.is_finite() { left } else { 0.0 };
            let right = if right.is_finite() { right } else { 0.0 };
            (left + right) * 0.5
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::decode_interleaved_stereo_f32;

    #[test]
    fn downmixes_stereo_float_loopback_samples() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1.0_f32.to_le_bytes());
        bytes.extend_from_slice(&-0.5_f32.to_le_bytes());
        assert_eq!(decode_interleaved_stereo_f32(&bytes), vec![0.25]);
    }
}
