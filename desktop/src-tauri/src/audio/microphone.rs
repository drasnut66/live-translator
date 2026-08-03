use super::AudioFrameSource;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, DeviceId, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig, I24, U24,
};
use live_translator_core::{AudioFrame, DEFAULT_AUDIO_FRAME_SAMPLES, WHISPER_SAMPLE_RATE};
use rubato::{FftFixedInOut, Resampler};
use serde::Serialize;
use std::collections::VecDeque;
use std::io;
use std::str::FromStr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::Duration;

const CAPTURE_QUEUE_SECONDS: usize = 5;
const RESAMPLER_CHUNK_HINT: usize = 1_024;
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum CaptureDeviceKind {
    Input,
}

impl CaptureDeviceKind {
    fn label(self) -> &'static str {
        "microphone"
    }
}

pub fn available_input_devices() -> Result<Vec<AudioDevice>, String> {
    available_devices(CaptureDeviceKind::Input)
}

pub(crate) fn available_devices(kind: CaptureDeviceKind) -> Result<Vec<AudioDevice>, String> {
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|device| device.id().ok());
    let devices = host
        .input_devices()
        .map_err(|error| format!("failed to enumerate {} devices: {error}", kind.label()))?;

    let mut result = Vec::new();
    for device in devices {
        let Ok(id) = device.id() else {
            continue;
        };
        let name = device
            .description()
            .map(|description| description.name().to_owned())
            .unwrap_or_else(|_| device.to_string());
        result.push(AudioDevice {
            is_default: default_id.as_ref() == Some(&id),
            id: id.to_string(),
            name,
        });
    }
    result.sort_by_key(|device| !device.is_default);
    Ok(result)
}

struct CaptureState {
    chunks: VecDeque<Vec<f32>>,
    queued_samples: usize,
    max_queued_samples: usize,
    dropped_samples: u64,
    error: Option<String>,
    stopped: bool,
}

struct SharedCapture {
    state: Mutex<CaptureState>,
    ready: Condvar,
}

impl SharedCapture {
    fn new(input_rate: u32) -> Self {
        Self {
            state: Mutex::new(CaptureState {
                chunks: VecDeque::new(),
                queued_samples: 0,
                max_queued_samples: input_rate as usize * CAPTURE_QUEUE_SECONDS,
                dropped_samples: 0,
                error: None,
                stopped: false,
            }),
            ready: Condvar::new(),
        }
    }

    fn push_samples(&self, samples: Vec<f32>) {
        if samples.is_empty() {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            if state.stopped {
                return;
            }
            state.queued_samples += samples.len();
            state.chunks.push_back(samples);
            while state.queued_samples > state.max_queued_samples {
                let Some(dropped) = state.chunks.pop_front() else {
                    break;
                };
                state.queued_samples -= dropped.len();
                state.dropped_samples += dropped.len() as u64;
            }
            self.ready.notify_one();
        }
    }

    fn fail(&self, error: String) {
        if let Ok(mut state) = self.state.lock() {
            state.error = Some(error);
            self.ready.notify_all();
        }
    }

    fn mark_stopped(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.stopped = true;
            self.ready.notify_all();
        }
    }
}

#[derive(Clone)]
pub(crate) struct CaptureSampleSink(Arc<SharedCapture>);

impl CaptureSampleSink {
    pub(crate) fn push_samples(&self, samples: Vec<f32>) {
        self.0.push_samples(samples);
    }

    pub(crate) fn fail(&self, error: impl Into<String>) {
        self.0.fail(error.into());
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn mark_stopped(&self) {
        self.0.mark_stopped();
    }
}

/// Turns native-rate mono chunks from a capture callback into the shared
/// 16 kHz, 20 ms `AudioFrame` contract.
pub(crate) struct BufferedPcmCapture {
    shared: Arc<SharedCapture>,
    processor: CaptureProcessor,
    ready_frames: VecDeque<AudioFrame>,
}

impl BufferedPcmCapture {
    pub(crate) fn new(input_rate: u32) -> io::Result<Self> {
        Ok(Self {
            shared: Arc::new(SharedCapture::new(input_rate)),
            processor: CaptureProcessor::new(input_rate)?,
            ready_frames: VecDeque::new(),
        })
    }

    pub(crate) fn sample_sink(&self) -> CaptureSampleSink {
        CaptureSampleSink(Arc::clone(&self.shared))
    }

    pub(crate) fn dropped_samples(&self) -> u64 {
        self.shared
            .state
            .lock()
            .map(|state| state.dropped_samples)
            .unwrap_or_default()
    }

    pub(crate) fn next_frame(&mut self, stop: &AtomicBool) -> io::Result<Option<AudioFrame>> {
        loop {
            if let Some(frame) = self.ready_frames.pop_front() {
                return Ok(Some(frame));
            }
            let Some(samples) = self.wait_for_samples(stop)? else {
                return Ok(None);
            };
            self.ready_frames.extend(self.processor.push(&samples)?);
        }
    }

    pub(crate) fn stop(&self) {
        self.shared.mark_stopped();
    }

    fn wait_for_samples(&self, stop: &AtomicBool) -> io::Result<Option<Vec<f32>>> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| io::Error::other("audio capture queue lock is poisoned"))?;
        loop {
            if let Some(error) = state.error.take() {
                return Err(io::Error::other(error));
            }
            if let Some(samples) = state.chunks.pop_front() {
                state.queued_samples -= samples.len();
                return Ok(Some(samples));
            }
            if state.stopped || stop.load(Ordering::Acquire) {
                return Ok(None);
            }
            let (next_state, _) = self
                .shared
                .ready
                .wait_timeout(state, STOP_POLL_INTERVAL)
                .map_err(|_| io::Error::other("audio capture queue lock is poisoned"))?;
            state = next_state;
        }
    }
}

pub(crate) struct CpalPcmSource {
    stream: Option<Stream>,
    capture: BufferedPcmCapture,
}

impl CpalPcmSource {
    pub(crate) fn open(device_id: Option<&str>, kind: CaptureDeviceKind) -> io::Result<Self> {
        let host = cpal::default_host();
        let device = match device_id {
            Some(id) => {
                let id = DeviceId::from_str(id).map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("invalid {} device ID: {error}", kind.label()),
                    )
                })?;
                host.device_by_id(&id).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("the selected {} is no longer available", kind.label()),
                    )
                })?
            }
            None => host.default_input_device().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("no {} device is available", kind.label()),
                )
            })?,
        };

        let supported_config = device.default_input_config().map_err(cpal_error)?;
        if supported_config.channels() == 0 || supported_config.sample_rate() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "the selected {} reported an invalid stream format",
                    kind.label()
                ),
            ));
        }

        let sample_format = supported_config.sample_format();
        let config = supported_config.config();
        let capture = BufferedPcmCapture::new(config.sample_rate)?;
        let stream = build_stream(
            &device,
            &config,
            sample_format,
            capture.sample_sink(),
            kind.label(),
        )?;
        stream.play().map_err(cpal_error)?;

        Ok(Self {
            stream: Some(stream),
            capture,
        })
    }

    pub(crate) fn dropped_samples(&self) -> u64 {
        self.capture.dropped_samples()
    }

    fn wait_for_frame(&mut self, stop: &AtomicBool) -> io::Result<Option<AudioFrame>> {
        self.capture.next_frame(stop)
    }

    fn close(&mut self) -> io::Result<()> {
        self.capture.stop();
        self.stream.take();
        Ok(())
    }
}

impl AudioFrameSource for CpalPcmSource {
    fn next_frame(&mut self, stop: &AtomicBool) -> io::Result<Option<AudioFrame>> {
        self.wait_for_frame(stop)
    }

    fn stop(&mut self) -> io::Result<()> {
        self.close()
    }
}

impl Drop for CpalPcmSource {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn build_stream(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    sink: CaptureSampleSink,
    source_label: &'static str,
) -> io::Result<Stream> {
    match format {
        SampleFormat::I8 => build_typed_stream::<i8>(device, config, sink, source_label),
        SampleFormat::I16 => build_typed_stream::<i16>(device, config, sink, source_label),
        SampleFormat::I24 => build_typed_stream::<I24>(device, config, sink, source_label),
        SampleFormat::I32 => build_typed_stream::<i32>(device, config, sink, source_label),
        SampleFormat::I64 => build_typed_stream::<i64>(device, config, sink, source_label),
        SampleFormat::U8 => build_typed_stream::<u8>(device, config, sink, source_label),
        SampleFormat::U16 => build_typed_stream::<u16>(device, config, sink, source_label),
        SampleFormat::U24 => build_typed_stream::<U24>(device, config, sink, source_label),
        SampleFormat::U32 => build_typed_stream::<u32>(device, config, sink, source_label),
        SampleFormat::U64 => build_typed_stream::<u64>(device, config, sink, source_label),
        SampleFormat::F32 => build_typed_stream::<f32>(device, config, sink, source_label),
        SampleFormat::F64 => build_typed_stream::<f64>(device, config, sink, source_label),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported {source_label} sample format: {format}"),
        )),
    }
}

fn build_typed_stream<T>(
    device: &Device,
    config: &StreamConfig,
    sink: CaptureSampleSink,
    source_label: &'static str,
) -> io::Result<Stream>
where
    T: SizedSample + Copy,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels);
    let error_sink = sink.clone();
    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data, _| {
                let samples =
                    downmix_interleaved(data, channels, |sample| f32::from_sample(sample));
                sink.push_samples(samples);
            },
            move |error| error_sink.fail(format!("{source_label} stream failed: {error}")),
            None,
        )
        .map_err(cpal_error)
}

pub struct MicrophonePcmSource(CpalPcmSource);

impl MicrophonePcmSource {
    pub fn open(device_id: Option<&str>) -> io::Result<Self> {
        CpalPcmSource::open(device_id, CaptureDeviceKind::Input).map(Self)
    }

    pub fn dropped_samples(&self) -> u64 {
        self.0.dropped_samples()
    }
}

impl AudioFrameSource for MicrophonePcmSource {
    fn next_frame(&mut self, stop: &AtomicBool) -> io::Result<Option<AudioFrame>> {
        self.0.next_frame(stop)
    }

    fn stop(&mut self) -> io::Result<()> {
        self.0.stop()
    }
}

fn cpal_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

fn downmix_interleaved<T: Copy>(
    input: &[T],
    channels: usize,
    mut to_f32: impl FnMut(T) -> f32,
) -> Vec<f32> {
    input
        .chunks_exact(channels)
        .map(|frame| frame.iter().copied().map(&mut to_f32).sum::<f32>() / channels as f32)
        .collect()
}

pub(crate) struct CaptureProcessor {
    resampler: Option<FftFixedInOut<f32>>,
    pending_input: VecDeque<f32>,
    pending_output: VecDeque<f32>,
    delay_to_skip: usize,
    samples_emitted: u64,
}

impl CaptureProcessor {
    pub(crate) fn new(input_rate: u32) -> io::Result<Self> {
        let resampler = if input_rate == WHISPER_SAMPLE_RATE {
            None
        } else {
            Some(
                FftFixedInOut::new(
                    input_rate as usize,
                    WHISPER_SAMPLE_RATE as usize,
                    RESAMPLER_CHUNK_HINT,
                    1,
                )
                .map_err(cpal_error)?,
            )
        };
        let delay_to_skip = resampler.as_ref().map(Resampler::output_delay).unwrap_or(0);
        Ok(Self {
            resampler,
            pending_input: VecDeque::new(),
            pending_output: VecDeque::new(),
            delay_to_skip,
            samples_emitted: 0,
        })
    }

    pub(crate) fn push(&mut self, input: &[f32]) -> io::Result<Vec<AudioFrame>> {
        if let Some(resampler) = self.resampler.as_mut() {
            self.pending_input.extend(input.iter().copied());
            let input_frames = resampler.input_frames_next();
            while self.pending_input.len() >= input_frames {
                let block = self.pending_input.drain(..input_frames).collect::<Vec<_>>();
                let mut output = resampler
                    .process(&[block], None)
                    .map_err(cpal_error)?
                    .pop()
                    .unwrap_or_default();
                let skip = self.delay_to_skip.min(output.len());
                output.drain(..skip);
                self.delay_to_skip -= skip;
                self.pending_output.extend(output);
            }
        } else {
            self.pending_output.extend(input.iter().copied());
        }

        let mut frames = Vec::new();
        while self.pending_output.len() >= DEFAULT_AUDIO_FRAME_SAMPLES {
            let timestamp_ms = self.samples_emitted * 1_000 / u64::from(WHISPER_SAMPLE_RATE);
            let samples = self
                .pending_output
                .drain(..DEFAULT_AUDIO_FRAME_SAMPLES)
                .collect::<Vec<_>>();
            self.samples_emitted += samples.len() as u64;
            frames.push(AudioFrame {
                samples,
                sample_rate: WHISPER_SAMPLE_RATE,
                channels: 1,
                timestamp_ms,
            });
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processor_preserves_samples_at_the_same_rate() {
        let mut processor = CaptureProcessor::new(16_000).expect("processor");
        let input = vec![0.25; DEFAULT_AUDIO_FRAME_SAMPLES];
        let frames = processor.push(&input).expect("frames");

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].samples, input);
    }

    #[test]
    fn fft_resampler_downsamples_with_a_bounded_startup_delay() {
        let mut processor = CaptureProcessor::new(48_000).expect("processor");
        let input = vec![0.5; 48_000];
        let frames = processor.push(&input).expect("frames");
        let samples = frames
            .iter()
            .map(|frame| frame.samples.len())
            .sum::<usize>();

        assert!((15_300..=16_000).contains(&samples), "samples={samples}");
        assert!(frames
            .iter()
            .flat_map(|frame| frame.samples.iter())
            .skip(DEFAULT_AUDIO_FRAME_SAMPLES)
            .all(|sample| (*sample - 0.5).abs() < 0.01));
    }

    #[test]
    fn downmix_and_processor_emit_twenty_millisecond_frames() {
        let mut stereo = Vec::with_capacity(DEFAULT_AUDIO_FRAME_SAMPLES * 2);
        for _ in 0..DEFAULT_AUDIO_FRAME_SAMPLES {
            stereo.extend([1.0_f32, -0.5]);
        }

        let mono = downmix_interleaved(&stereo, 2, |sample| sample);
        let mut processor = CaptureProcessor::new(16_000).expect("processor");
        let frames = processor.push(&mono).expect("frames");

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].samples.len(), DEFAULT_AUDIO_FRAME_SAMPLES);
        assert!(frames[0]
            .samples
            .iter()
            .all(|sample| (*sample - 0.25).abs() < f32::EPSILON));
        assert_eq!(frames[0].timestamp_ms, 0);
    }

    #[test]
    fn raw_capture_queue_drops_old_audio_after_five_seconds() {
        let shared = SharedCapture::new(10);
        shared.push_samples(vec![0.0; 30]);
        shared.push_samples(vec![1.0; 30]);

        let state = shared.state.lock().expect("capture state");
        assert_eq!(state.queued_samples, 30);
        assert_eq!(state.dropped_samples, 30);
        assert_eq!(state.chunks.len(), 1);
    }
}
