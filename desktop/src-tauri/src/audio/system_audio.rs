#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::{available_system_audio_devices, SystemAudioPcmSource};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use unsupported::{available_system_audio_devices, SystemAudioPcmSource};
#[cfg(target_os = "windows")]
pub use windows::{available_system_audio_devices, SystemAudioPcmSource};
