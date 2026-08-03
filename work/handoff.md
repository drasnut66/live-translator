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
- Added a long-lived, statically linked whisper.cpp engine through `whisper-rs`, with
  Metal enabled on macOS and the same CPU-capable API on Windows.
- Verified repeated Japanese revisions and final stable-prefix output against the first
  eight seconds of `1.flv` using `examples/live_asr_probe.rs`.
- Added the Tauri 2 + React desktop overlay and connected it to Rust runtime status.
- Added a managed Tauri recognition session with `session:status`, `subtitle:partial`,
  and `subtitle:commit` events, including real-time playback from `1.flv`.
- Added a bounded asynchronous OpenAI-compatible translation worker for committed
  Japanese text; it is disabled automatically when no API key is configured.
- Added rolling Silero VAD endpoint detection to the managed session, with final decode
  on silence and a 15-second fallback for continuous speech.
- Verified four VAD endpoints in the first 30 seconds of `1.flv` using
  `examples/vad_probe.rs`.

## Environment status

- CMake and Apple Clang are available.
- Node.js and npm are available.
- Rust 1.97.1 and Cargo 1.97.1 are available.
- FFmpeg 8.1.2 is available.
- whisper.cpp was built with Metal in the local development checkout.
- `ggml-large-v3-turbo-q5_0.bin` and `ggml-silero-v6.2.0.bin` are present.
- Core tests, the production web build, and the Tauri Rust check pass.

## Next executable tasks

1. Add microphone capture behind the shared 16 kHz mono frame contract.
2. Isolate WASAPI and ScreenCaptureKit system-audio adapters behind that same contract.
3. Move API keys into the OS credential store and expose provider settings in the UI.
4. Add DeepL and local NLLB/CTranslate2 providers behind the same translation worker.

## Current blockers

There is no active toolchain blocker. The media-input pipeline, managed session,
VAD-driven segmentation, Tauri events, and optional API translation are connected. The
remaining functional gap is live device capture. The `.flv` extension is misleading;
FFmpeg identifies `1.flv` as MP4/MOV, so all media paths must continue to probe content
rather than trust the suffix.

The macOS transparent Tauri window enables `macOSPrivateApi`; this is acceptable for
personal side-loading but incompatible with Mac App Store submission.
