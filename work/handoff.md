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
- Added cross-platform CPAL microphone capture with input-device enumeration, native sample
  conversion, channel downmix, Rubato FFT resampling to 16 kHz, 20 ms frames, and a bounded
  five-second raw capture queue.
- Connected microphone selection and session start to the Tauri overlay, and added the
  macOS microphone usage description required for packaged permission prompts.
- Verified a two-second capture from the MacBook Air microphone: 2000 ms of PCM, peak
  0.0185, and zero dropped native samples.
- Replaced the CPAL output-device placeholder with platform system-audio adapters behind the
  common `AudioFrameSource`: ScreenCaptureKit on macOS 13+ and WASAPI render loopback on
  Windows. Both paths enter the existing 48-to-16 kHz processing and bounded queue contract.
- Added the macOS Screen Recording usage description and changed the packaged macOS baseline
  to 13.0, which is required by ScreenCaptureKit system-audio capture.
- Ran the macOS system-audio probe against the built-in display for 5 seconds while playing a
  20-second 880 Hz fixture: it emitted 5000 ms with zero dropped samples but `peak=0.0000`.
  The command-line executable still needs an explicit Screen Recording grant, or validation
  must run through the packaged Live Translator app, before this can count as non-silent proof.
- Added OpenAI-compatible provider settings to the overlay. Endpoint and model persist in the
  application data directory; API keys write only to macOS Keychain or Windows Credential
  Manager and are never returned to the webview or written to JSON.
- Kept `LIVE_TRANSLATOR_API_*` environment variables as the development fallback. A stored
  credential takes precedence when a recognition session creates its translation worker.
- Added a DeepL provider to the same bounded translation worker. It uses a separate credential
  account, sends `POST /v2/translate` JSON with the `DeepL-Auth-Key` header, and targets `ZH`.
- Assessed local NLLB/CTranslate2 availability: no runtime package or model assets are present
  in this workspace, so local-provider integration remains a model-distribution decision.
- Verified macOS system-audio capture with audible output: after granting Screen Recording
  permission to the probe binary, an `ffplay`-played 880 Hz fixture produced 5000 ms with
  `peak=0.0885` and zero dropped samples. `afplay` did not route into the SCK loopback on
  this machine, so playback-path choice matters during validation. See
  `benchmark-2026-08-03.md`.

## Environment status

- CMake and Apple Clang are available.
- Node.js and npm are available.
- Rust 1.97.1 and Cargo 1.97.1 are available.
- FFmpeg 8.1.2 is available.
- whisper.cpp was built with Metal in the local development checkout.
- `ggml-large-v3-turbo-q5_0.bin` and `ggml-silero-v6.2.0.bin` are present.
- Core tests, the production web build, and the Tauri Rust check pass. macOS system-audio
  capture is verified non-silent; Windows WASAPI hardware validation remains pending.

## Next executable tasks

1. Run the equivalent WASAPI hardware probe on a Windows host.
2. Run the full soak test and record the latency/memory results in `test-plan.md`.
   The soak probe (`examples/soak_probe.rs`) and streaming wall-latency fields on the
   partial/commit/translation events are in place; the cloud-provider decision (no local
   NLLB) is recorded as D015.

## Current blockers

There is no active toolchain blocker. The media-input pipeline, managed session,
VAD-driven segmentation, Tauri events, optional API translation, microphone capture, and
macOS system-audio capture are connected and verified. Windows system-audio hardware
validation is pending a Windows host. The remaining audio-source gap is browser-tab
capture. The `.flv` extension is misleading; FFmpeg identifies `1.flv` as MP4/MOV, so all
media paths must continue to probe content rather than trust the suffix.

The macOS transparent Tauri window enables `macOSPrivateApi`; this is acceptable for
personal side-loading but incompatible with Mac App Store submission.
