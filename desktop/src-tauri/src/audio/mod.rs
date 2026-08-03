pub mod ffmpeg;
pub mod microphone;
pub mod system_audio;
pub mod vad;
pub mod websocket;

use live_translator_core::AudioFrame;
use std::io;
use std::sync::atomic::AtomicBool;

pub trait AudioFrameSource {
    fn next_frame(&mut self, stop: &AtomicBool) -> io::Result<Option<AudioFrame>>;
    fn stop(&mut self) -> io::Result<()>;
}
