use super::AudioFrameSource;
use live_translator_core::{AudioFrame, Pcm16FrameAssembler};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone)]
pub struct FfmpegSourceConfig {
    pub program: PathBuf,
    pub input: String,
    pub realtime: bool,
}

impl FfmpegSourceConfig {
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            program: configured_ffmpeg_program(),
            input: input.into(),
            realtime: true,
        }
    }

    fn args(&self) -> Vec<OsString> {
        let mut args = Vec::with_capacity(15);
        args.push(OsString::from("-hide_banner"));
        args.push(OsString::from("-loglevel"));
        args.push(OsString::from("error"));
        args.push(OsString::from("-nostdin"));
        if self.realtime {
            args.push(OsString::from("-re"));
        }
        args.push(OsString::from("-i"));
        args.push(OsString::from(&self.input));
        args.push(OsString::from("-map"));
        args.push(OsString::from("0:a:0"));
        args.push(OsString::from("-vn"));
        args.push(OsString::from("-ac"));
        args.push(OsString::from("1"));
        args.push(OsString::from("-ar"));
        args.push(OsString::from("16000"));
        args.push(OsString::from("-acodec"));
        args.push(OsString::from("pcm_s16le"));
        args.push(OsString::from("-f"));
        args.push(OsString::from("s16le"));
        args.push(OsString::from("pipe:1"));
        args
    }
}

pub fn configured_ffmpeg_program() -> PathBuf {
    std::env::var_os("LIVE_TRANSLATOR_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

/// Incremental FFmpeg decoder for files, HLS/RTSP URLs, and other FFmpeg
/// inputs. It emits the same 16 kHz mono `AudioFrame` contract as native
/// microphone/system-audio adapters will use later.
pub struct FfmpegPcmSource {
    child: Child,
    stdout: ChildStdout,
    assembler: Pcm16FrameAssembler,
    ready_frames: VecDeque<AudioFrame>,
    finished: bool,
}

impl FfmpegPcmSource {
    pub fn spawn(config: FfmpegSourceConfig) -> io::Result<Self> {
        let mut command = Command::new(&config.program);
        command
            .args(config.args())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }

        let mut child = command.spawn().map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("failed to start FFmpeg {:?}: {error}", config.program),
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "FFmpeg stdout was not piped")
        })?;

        Ok(Self {
            child,
            stdout,
            assembler: Pcm16FrameAssembler::whisper_defaults(),
            ready_frames: VecDeque::new(),
            finished: false,
        })
    }

    /// Blocks until the next 20 ms frame is decoded or the source ends.
    pub fn next_frame(&mut self) -> io::Result<Option<AudioFrame>> {
        loop {
            if let Some(frame) = self.ready_frames.pop_front() {
                return Ok(Some(frame));
            }
            if self.finished {
                return Ok(None);
            }

            let mut buffer = [0_u8; 64 * 1024];
            let bytes_read = self.stdout.read(&mut buffer)?;
            if bytes_read == 0 {
                if let Some(frame) = self.assembler.finish() {
                    self.ready_frames.push_back(frame);
                }
                let status = self.child.wait()?;
                self.finished = true;
                if !status.success() {
                    return Err(io::Error::other(format!(
                        "FFmpeg exited with status {status}"
                    )));
                }
                continue;
            }

            self.ready_frames
                .extend(self.assembler.push(&buffer[..bytes_read]));
        }
    }

    pub fn stop(&mut self) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }
        self.child.kill()?;
        self.child.wait()?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for FfmpegPcmSource {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl AudioFrameSource for FfmpegPcmSource {
    fn next_frame(&mut self, _stop: &AtomicBool) -> io::Result<Option<AudioFrame>> {
        FfmpegPcmSource::next_frame(self)
    }

    fn stop(&mut self) -> io::Result<()> {
        FfmpegPcmSource::stop(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realtime_args_keep_stream_decode_incremental() {
        let config = FfmpegSourceConfig::new("input.mp4");
        let args = config
            .args()
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(args.windows(2).any(|pair| pair == ["-re", "-i"]));
        assert!(args.windows(2).any(|pair| pair == ["-ar", "16000"]));
        assert!(args.windows(2).any(|pair| pair == ["-f", "s16le"]));
        assert_eq!(args.last().map(String::as_str), Some("pipe:1"));
    }

    #[test]
    fn realtime_can_be_disabled_for_already_live_network_inputs() {
        let mut config = FfmpegSourceConfig::new("https://example.test/live.m3u8");
        config.realtime = false;
        let args = config
            .args()
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(!args.iter().any(|value| value == "-re"));
    }
}
