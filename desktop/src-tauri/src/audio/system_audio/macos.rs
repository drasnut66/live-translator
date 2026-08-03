use crate::audio::microphone::{AudioDevice, BufferedPcmCapture};
use crate::audio::AudioFrameSource;
use live_translator_core::AudioFrame;
use screencapturekit::prelude::*;
use std::io;
use std::sync::atomic::AtomicBool;

const SAMPLE_RATE: u32 = 48_000;
const DISPLAY_ID_PREFIX: &str = "screencapturekit:";

pub fn available_system_audio_devices() -> Result<Vec<AudioDevice>, String> {
    let content = SCShareableContent::get().map_err(screen_capture_error)?;
    let displays = content.displays();
    if displays.is_empty() {
        return Err(screen_capture_error(
            "no shareable displays were returned by ScreenCaptureKit",
        ));
    }
    let mut displays = displays
        .into_iter()
        .enumerate()
        .map(|(index, display)| AudioDevice {
            id: format!("{DISPLAY_ID_PREFIX}{}", display.display_id()),
            name: format!(
                "显示器 {} ({}x{})",
                display.display_id(),
                display.width(),
                display.height()
            ),
            is_default: index == 0,
        })
        .collect::<Vec<_>>();
    displays.sort_by_key(|device| !device.is_default);
    Ok(displays)
}

pub struct SystemAudioPcmSource {
    stream: Option<SCStream>,
    capture: BufferedPcmCapture,
}

impl SystemAudioPcmSource {
    pub fn open(device_id: Option<&str>) -> io::Result<Self> {
        let content = SCShareableContent::get().map_err(screen_capture_io_error)?;
        let displays = content.displays();
        if displays.is_empty() {
            return Err(screen_capture_io_error(
                "no shareable displays were returned by ScreenCaptureKit",
            ));
        }
        let requested_display_id = parse_display_id(device_id)?;
        let display = displays
            .into_iter()
            .find(|display| requested_display_id.is_none_or(|id| display.display_id() == id))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "the selected display is no longer available for system audio capture",
                )
            })?;

        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();
        let config = SCStreamConfiguration::new()
            .with_width(display.width())
            .with_height(display.height())
            .with_captures_audio(true)
            .with_excludes_current_process_audio(true)
            .with_sample_rate(SAMPLE_RATE as i32)
            .with_channel_count(1);
        let capture = BufferedPcmCapture::new(SAMPLE_RATE)?;
        let error_sink = capture.sample_sink();
        let mut stream = SCStream::new_with_delegate(
            &filter,
            &config,
            ErrorHandler::new(move |error| {
                error_sink.fail(format!("ScreenCaptureKit stopped: {error}"));
            }),
        );
        let audio_sink = capture.sample_sink();
        if stream
            .add_output_handler(
                move |sample: CMSampleBuffer, output_type| {
                    if output_type != SCStreamOutputType::Audio {
                        return;
                    }
                    match decode_mono_f32(sample) {
                        Ok(samples) => audio_sink.push_samples(samples),
                        Err(error) => audio_sink.fail(error),
                    }
                },
                SCStreamOutputType::Audio,
            )
            .is_none()
        {
            return Err(io::Error::other(
                "ScreenCaptureKit refused to register the system audio output handler",
            ));
        }
        stream.start_capture().map_err(screen_capture_io_error)?;

        Ok(Self {
            stream: Some(stream),
            capture,
        })
    }

    pub fn dropped_samples(&self) -> u64 {
        self.capture.dropped_samples()
    }

    fn close(&mut self) -> io::Result<()> {
        self.capture.stop();
        if let Some(stream) = self.stream.take() {
            stream.stop_capture().map_err(screen_capture_io_error)?;
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

fn parse_display_id(device_id: Option<&str>) -> io::Result<Option<u32>> {
    device_id
        .map(|value| {
            value
                .strip_prefix(DISPLAY_ID_PREFIX)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid ScreenCaptureKit display identifier",
                    )
                })?
                .parse::<u32>()
                .map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("invalid ScreenCaptureKit display identifier: {error}"),
                    )
                })
        })
        .transpose()
}

fn decode_mono_f32(sample: CMSampleBuffer) -> Result<Vec<f32>, String> {
    let buffers = sample
        .audio_buffer_list()
        .ok_or_else(|| "ScreenCaptureKit delivered an audio sample without PCM data".to_owned())?;
    let mut samples = Vec::new();
    for buffer in &buffers {
        if buffer.number_channels != 1 {
            return Err(format!(
                "ScreenCaptureKit returned {} audio channels despite requesting mono",
                buffer.number_channels
            ));
        }
        let bytes = buffer.data();
        if bytes.len() % std::mem::size_of::<f32>() != 0 {
            return Err("ScreenCaptureKit returned a malformed float PCM buffer".to_owned());
        }
        samples.extend(bytes.chunks_exact(4).map(|chunk| {
            let value = f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if value.is_finite() {
                value
            } else {
                0.0
            }
        }));
    }
    Ok(samples)
}

fn screen_capture_error(error: impl std::fmt::Display) -> String {
    format!(
        "ScreenCaptureKit requires Screen Recording permission. Enable it for Live Translator in System Settings > Privacy & Security > Screen Recording, then restart the app: {error}"
    )
}

fn screen_capture_io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, screen_capture_error(error))
}

#[cfg(test)]
mod tests {
    use super::parse_display_id;

    #[test]
    fn parses_a_screencapturekit_display_identifier() {
        assert_eq!(
            parse_display_id(Some("screencapturekit:42")).expect("display ID"),
            Some(42)
        );
        assert!(parse_display_id(Some("coreaudio:output")).is_err());
    }
}
