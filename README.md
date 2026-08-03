# Live Translator

Cross-platform Japanese-to-Chinese live subtitles for personal use.

This repository is the shared Mac/Windows application shell for a Dango-like compact
overlay. It combines a Tauri 2 desktop UI with a local Whisper backend and a pluggable
translation provider. The codebase is intentionally small and keeps large models,
platform binaries, and build caches outside Git.

> Current status: the desktop overlay, incremental FFmpeg PCM source, in-process
> long-lived whisper.cpp engine, stable-prefix streaming core, managed Tauri session, and
> asynchronous OpenAI-compatible translation worker are implemented. Silero VAD and
> microphone/system-audio capture are the next milestone.

## Features in this milestone

- Tauri 2 + React/TypeScript compact always-on-top overlay for macOS and Windows.
- Japanese/Chinese dual subtitle layout with partial Japanese text styling.
- Audio source selector UI for browser, system audio, and microphone paths.
- Pause, subtitle history, provider selection, and opacity controls.
- whisper.cpp + Metal local ASR smoke test with Silero VAD.
- Incremental FFmpeg source for files, HLS/RTSP URLs, and other FFmpeg inputs.
- 20 ms PCM frame assembly and bounded audio queue with dropped-frame accounting.
- Statically linked whisper.cpp engine with Metal on macOS and CPU fallback on Windows.
- Token-prefix stability coordinator with duplicate and regression protection.
- Managed Tauri session events for streaming Japanese partials and commits.
- Optional asynchronous OpenAI-compatible translation for committed Japanese text.
- GitHub Actions checks the core on macOS, Windows, and Linux, with full desktop checks
  on macOS and Windows.

## Architecture

```text
audio adapter
  -> 16 kHz mono PCM
  -> bounded queue + Silero VAD
  -> long-lived whisper.cpp worker
  -> stable-prefix coordinator
  -> Japanese subtitle event
  -> translation provider (OpenAI-compatible / DeepL / local)
  -> Tauri overlay
```

The same Tauri/React shell is used on both desktop platforms. Platform-specific audio
capture is isolated behind one internal frame contract: browser-tab capture first,
microphone next, then WASAPI loopback on Windows and ScreenCaptureKit on macOS.

## Repository layout

- `core/`: backend-neutral streaming coordination crate.
- `desktop/`: Tauri 2 + React desktop application.
- `scripts/`: local media smoke tests and model setup helpers.
- `work/`: handoff notes, architecture decisions, and benchmarks.

The downloaded reference repositories (`whisper-live-kit`, `whisper.cpp`, and Dango)
are kept outside this repository under the parent `third_party/` directory. They are
not vendored into the published project.

## Development prerequisites

- Node.js 20 or newer and npm.
- Rust stable and Cargo.
- FFmpeg 6 or newer for media smoke tests.
- An optional local `whisper-cli` build for the batch smoke-test script.
- The Whisper and Silero model files described below.

### Install dependencies

```sh
cd desktop
npm ci
cd ../core
cargo test
```

### Start the desktop shell

```sh
cd desktop
npm run tauri dev
```

Build checks:

```sh
cd desktop
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cd ../core
cargo test
```

### Local model directory

Models are deliberately not committed to GitHub. Put these files in `models/` for a
development checkout:

- `ggml-large-v3-turbo-q5_0.bin`
- `ggml-silero-v6.2.0.bin`

The runtime also accepts `LIVE_TRANSLATOR_MODEL_DIR` to point at an alternate directory.
This is the preferred installation path for packaged applications on both Mac and
Windows. Never commit API keys or model binaries.

### Media smoke test

```sh
./scripts/transcribe_media.sh /path/to/japanese-video.mp4
```

The script probes the actual container, decodes the first audio stream to 16 kHz mono,
and writes text/SRT/JSON next to the repository-level working notes. It remains a batch
benchmark. The native FFmpeg source in `desktop/src-tauri/src/audio/ffmpeg.rs` performs
the decode incrementally for the in-process ASR engine.

### Incremental ASR probe

The development probe exercises FFmpeg, the in-process Whisper engine, and stable-prefix
revisions against a media file without loading the model for every revision:

```sh
cargo run --manifest-path desktop/src-tauri/Cargo.toml --example live_asr_probe -- \
  models/ggml-large-v3-turbo-q5_0.bin /path/to/japanese-video.mp4 12
```

The `whisper-rs` wrapper statically compiles whisper.cpp, enables Metal on macOS, and
uses the same Rust API with a CPU backend on Windows.

## Translation providers

The current provider is an OpenAI-compatible `/v1/chat/completions` client. It runs in a
separate bounded worker, uses a request timeout, and accepts compatible gateways such as
local OpenAI-compatible servers by changing the endpoint. Configure it in the environment
before starting the desktop app:

```sh
export LIVE_TRANSLATOR_API_KEY="..."
export LIVE_TRANSLATOR_API_URL="https://api.openai.com/v1/chat/completions"
export LIVE_TRANSLATOR_MODEL="gpt-4o-mini"
```

Optional `LIVE_TRANSLATOR_SYSTEM_PROMPT` and `LIVE_TRANSLATOR_TIMEOUT_MS` are supported.
When the key is absent, the app remains local-only and shows `等待翻译` for committed
lines. API keys are not stored in Git, JSON settings, or SQLite; OS credential-store
integration is still planned for packaged builds. DeepL and local NLLB providers can be
added behind the same worker contract later.

## Cross-platform release

Push a version tag such as `v0.1.0` to run the release workflow. GitHub Actions builds
native artifacts on macOS and Windows through the official Tauri action. Signing and
notarization secrets are intentionally not included; unsigned personal builds are the
initial target.

The macOS overlay enables Tauri's `macOSPrivateApi` because true transparent windows
need it. This is suitable for personal side-loading but is not compatible with Mac App
Store submission. The minimum macOS version is 10.15 because the statically linked
whisper.cpp runtime uses C++ filesystem APIs introduced there. Windows uses the same
source tree and does not require the private API.

## License and references

The application code is released under the MIT License. Upstream projects remain under
their own licenses and are reference material, not copied source in this repository.
The distributed ASR binary includes third-party code listed in `THIRD_PARTY_NOTICES.md`.
See `work/decisions.md` for the boundary decisions and `work/handoff.md` for the next
implementation tasks.
