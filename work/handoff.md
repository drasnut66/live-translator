# Handoff

Date: 2026-08-03

## Completed

- Evaluated WhisperLiveKit, whisper.cpp, and Dango as external references.
- Created `live-translator/` for project code.
- Created this `work/` directory for handoff notes.
- Confirmed `1.flv` is present and unchanged.
- Installed Rust/Cargo and FFmpeg; verified Node, CMake, and Apple Clang.
- Built whisper.cpp with Metal support.
- Downloaded and checksum-recorded the quantized Whisper and Silero VAD models.
- Added `scripts/transcribe_media.sh` and completed a full-file Japanese ASR run.
- Added the Rust stable-prefix coordinator with regression and duplicate protection.
- Added incremental `s16le` PCM frame assembly, bounded queue accounting, and a
  cross-platform FFmpeg source that reads files and network streams without creating
  intermediate WAV files.
- Added the Tauri 2 + React desktop overlay and connected it to Rust runtime status.

## Environment status

- CMake and Apple Clang are available.
- Node.js and npm are available.
- Rust 1.97.1 and Cargo 1.97.1 are available.
- FFmpeg 8.1.2 is available.
- whisper.cpp was built with Metal in the local development checkout.
- `ggml-large-v3-turbo-q5_0.bin` and `ggml-silero-v6.2.0.bin` are present.
- Core tests, the production web build, and the Tauri Rust check pass.

## Next executable tasks

1. Add the long-lived whisper.cpp FFI worker and connect it to the bounded audio queue.
2. Feed ASR revisions through the stable-prefix coordinator into Tauri events.
3. Implement the OpenAI-compatible translation provider with cancellation and timeout.
4. Add microphone capture, then isolate WASAPI and ScreenCaptureKit system-audio adapters.

## Current blockers

There is no active toolchain blocker. The main engineering gap is the live audio-to-ASR
worker: the current script is batch-oriented even though inference is faster than real
time. The `.flv` extension is misleading; FFmpeg identifies `1.flv` as MP4/MOV, so all
media paths must continue to probe content rather than trust the suffix.

The macOS transparent Tauri window enables `macOSPrivateApi`; this is acceptable for
personal side-loading but incompatible with Mac App Store submission.
