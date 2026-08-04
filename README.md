# Live Translator

Cross-platform Japanese-to-Chinese live subtitles for personal use.

This repository is the shared Mac/Windows application shell for a Dango-like compact
overlay. It combines a Tauri 2 desktop UI with a local Whisper backend and a pluggable
translation provider. The codebase is intentionally small and keeps large models,
platform binaries, and build caches outside Git.

> Current status: the desktop overlay, incremental FFmpeg PCM source, in-process
> long-lived whisper.cpp engine, stable-prefix streaming core, managed Tauri session, and
> asynchronous OpenAI-compatible translation worker are implemented. Silero VAD now
> closes utterances on detected silence, and microphone and system-audio capture feed the
> same native session on their supported platforms.

## Features in this milestone

- Tauri 2 + React/TypeScript compact always-on-top overlay for macOS and Windows.
- Japanese/Chinese dual subtitle layout with partial Japanese text styling.
- Audio source selector UI for browser, system audio, and microphone paths.
- Pause, subtitle history, provider selection, and opacity controls.
- whisper.cpp + Metal local ASR smoke test with Silero VAD.
- Incremental FFmpeg source for files, HLS/RTSP URLs, and other FFmpeg inputs.
- 20 ms PCM frame assembly and bounded audio queue with dropped-frame accounting.
- CPAL microphone capture with device selection, native-format conversion, mono downmix,
  anti-aliased 16 kHz resampling, and a bounded five-second capture queue.
- ScreenCaptureKit system-audio capture on macOS 13+ with display selection and a bounded
  native capture queue; WASAPI render-loopback capture on Windows.
- Statically linked whisper.cpp engine with Metal on macOS and CPU fallback on Windows.
- Token-prefix stability coordinator with duplicate and regression protection.
- Managed Tauri session events for streaming Japanese partials and commits.
- Silero VAD endpointing with a 15-second continuous-speech safety boundary.
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
capture is isolated behind one internal frame contract. Media, microphone, system-audio,
and browser-tab (via the bundled Chrome extension) sources are implemented.

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
development checkout. The runtime needs whisper-small by default and falls back to
large-v3-turbo when small is absent; Silero VAD is always required:

- `ggml-small.bin` — default streaming model (reaches 1.0x realtime on Apple Silicon)
- `ggml-large-v3-turbo-q5_0.bin` — optional, higher-accuracy offline runs
- `ggml-silero-v6.2.0.bin` — required VAD model

`scripts/download_models.sh` (or `download_models.ps1` on Windows) fetches these plus
the optional `speakrs/` (speaker diarization) and `hymt/` (offline translation) models
with checksum verification. The offline translation provider also needs a `llama-server`
binary (or `llama-server.exe` on Windows) next to the Hy-MT model under `models/hymt/`;
grab it from a recent [llama.cpp release](https://github.com/ggml-org/llama.cpp/releases).
Set `LIVE_TRANSLATOR_WHISPER_MODEL` to a filename in the model directory (for example
`ggml-large-v3-turbo-q5_0.bin`) to override the default model. The runtime also accepts
`LIVE_TRANSLATOR_MODEL_DIR` to point at an alternate directory. This is the preferred
installation path for packaged applications on both Mac and Windows. Never commit API
keys or model binaries.

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
  models/ggml-small.bin /path/to/japanese-video.mp4 12
```

The `whisper-rs` wrapper statically compiles whisper.cpp, enables Metal on macOS, and
uses the same Rust API with a CPU backend on Windows.

### VAD endpoint probe

This probe runs the same Silero VAD adapter used by the managed session without loading
Whisper, making endpoint tuning quick:

```sh
cargo run --manifest-path desktop/src-tauri/Cargo.toml --example vad_probe -- \
  models/ggml-silero-v6.2.0.bin /path/to/japanese-video.mp4 30
```

### Microphone probe

This probe enumerates input devices and verifies the 16 kHz mono frame path without
loading Whisper. The first argument is the capture duration; an optional second argument
is a device ID printed by the probe:

```sh
cargo run --manifest-path desktop/src-tauri/Cargo.toml --example microphone_probe -- 3
```

macOS asks for microphone permission on first use. Packaged builds include the required
`NSMicrophoneUsageDescription`; Windows uses the same CPAL adapter and device contract.

### System-audio probe

The system-audio probe verifies the same 16 kHz mono frame path without loading Whisper:

```sh
cargo run --manifest-path desktop/src-tauri/Cargo.toml --example system_audio_probe -- 3
```

On macOS 13 or newer, select a display and grant Screen Recording permission when macOS asks.
The application excludes its own output to avoid feedback. On Windows, the optional second
argument is the WASAPI render-endpoint ID printed by the probe.

### Browser-tab capture

A Chrome MV3 extension (`browser-tab-capture/`) captures the audio of the active tab and
streams it to the app over a localhost WebSocket. This is the cleanest source for web
video because it captures only the target tab (no system notifications or other app audio,
no microphone feedback).

Setup:

1. Load the unpacked `browser-tab-capture/` directory in Chrome at `chrome://extensions`
   (enable Developer mode).
2. Pin the "Live Translator Tab Capture" extension.
3. In the app, choose "浏览器音频" as the audio source and start the session.
4. Click the extension icon on the tab you want to translate. The badge shows `ON` while
   capturing; click again to stop.

The extension sends raw little-endian f32 PCM at 16 kHz mono over `ws://127.0.0.1:8765/`.
`scripts/websocket_client_sim.py` can simulate the extension for development; the
`browser_tab_probe` example verifies the WebSocket source path without loading Whisper:

```sh
cargo run --manifest-path desktop/src-tauri/Cargo.toml --example browser_tab_probe -- 5
python3 scripts/websocket_client_sim.py 4
```

## Translation providers

OpenAI-compatible and DeepL providers run in the same separate bounded worker and use a
request timeout. OpenAI-compatible gateways can be selected by changing the endpoint; DeepL
defaults to the free API endpoint and also accepts a custom endpoint for Pro accounts. Open
the overlay settings to select a provider and set its endpoint and API key. The OpenAI
provider also exposes its model field.

Keys are written directly to macOS Keychain or Windows Credential Manager and are never
returned to the webview, placed in JSON settings, or written to SQLite. Endpoint and model
are stored in the application data directory and take effect when the next recognition
session starts.

Development builds can still use environment variables before a key has been saved in the
credential store:

```sh
export LIVE_TRANSLATOR_API_KEY="..."
export LIVE_TRANSLATOR_API_URL="https://api.openai.com/v1/chat/completions"
export LIVE_TRANSLATOR_MODEL="gpt-4o-mini"
# Optional DeepL fallback when the selected provider is DeepL.
export LIVE_TRANSLATOR_DEEPL_API_KEY="..."
```

Optional `LIVE_TRANSLATOR_SYSTEM_PROMPT` and `LIVE_TRANSLATOR_TIMEOUT_MS` are supported.
When the key is absent, the app remains local-only and shows `等待翻译` for committed
lines. A saved OS credential takes precedence over the matching environment variable. A local
NLLB runtime was considered and deliberately excluded for the initial product (see D015 in
`work/decisions.md`); the app stays cloud-only for translation to keep the build lightweight.

## Cross-platform release

Push a version tag such as `v0.1.0` to run the release workflow. GitHub Actions builds
native artifacts on macOS and Windows through the official Tauri action. Signing and
notarization secrets are intentionally not included; unsigned personal builds are the
initial target.

The macOS overlay enables Tauri's `macOSPrivateApi` because true transparent windows
need it. This is suitable for personal side-loading but is not compatible with Mac App
Store submission. The minimum macOS version is 13.0 because ScreenCaptureKit system-audio
capture is available from that release. Windows uses the same source tree and does not
require the private API.

## License and references

The application code is released under the MIT License. Upstream projects remain under
their own licenses and are reference material, not copied source in this repository.
The distributed ASR binary includes third-party code listed in `THIRD_PARTY_NOTICES.md`.
See `work/decisions.md` for the boundary decisions and `work/handoff.md` for the next
implementation tasks.
