use crate::audio::microphone::AudioDevice;
use crate::audio::AudioFrameSource;
use live_translator_core::AudioFrame;
use std::io;
use std::sync::atomic::AtomicBool;

pub fn available_system_audio_devices() -> Result<Vec<AudioDevice>, String> {
    Ok(Vec::new())
}

pub struct SystemAudioPcmSource;

impl SystemAudioPcmSource {
    pub fn open(_device_id: Option<&str>) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "system audio capture is supported only on macOS and Windows",
        ))
    }

    pub fn dropped_samples(&self) -> u64 {
        0
    }
}

impl AudioFrameSource for SystemAudioPcmSource {
    fn next_frame(&mut self, _stop: &AtomicBool) -> io::Result<Option<AudioFrame>> {
        Ok(None)
    }

    fn stop(&mut self) -> io::Result<()> {
        Ok(())
    }
}
