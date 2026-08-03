# Decision Log

## D001: Do not fork Dango as the application base

Dango's public repository is an OCR-first Windows application. Its README states that
the current application was rewritten in Go after 4.5.8 and that the Python source is
no longer updated. We use it as an interaction and provider-design reference only.

## D002: Keep the product shell separate from inference

The project owns the subtitle UI and orchestration. The ASR engine is behind an internal
interface so whisper.cpp can be replaced by another local backend without changing the
UI or translation provider code.

## D003: Prefer browser-tab capture for web video

It is the most consistent input path on macOS and Windows. Native system-audio capture
remains available, but is isolated behind platform adapters and permissions.

## D004: Translate committed text

Japanese is frequently verb-final. Translating every partial result produces wrong,
unstable Chinese and excessive API traffic. The default policy waits for punctuation,
VAD silence, or a bounded sentence length.

## D005: Do not download models into the repository

Models are large and platform-independent. They will be downloaded to an application
model directory with checksum verification and resumable downloads.

## D006: Allow the macOS transparent-window private API for personal builds

The overlay needs true window transparency, so `app.macOSPrivateApi` is enabled in the
Tauri configuration. This prevents Mac App Store acceptance. The target is personal
side-loaded use; if distribution goals change, replace the transparent native window
before attempting an App Store release.

## D007: Statically link whisper.cpp through a pinned Rust wrapper

The desktop backend pins `whisper-rs` 0.16.0 instead of controlling `whisper-cli` as a
long-running subprocess. This keeps model state in the application process, exposes
token IDs for stable-prefix comparison, and produces one API on macOS and Windows.
Metal is compiled on macOS; Windows initially uses CPU inference. The wrapper is
Unlicense and its bundled whisper.cpp code is MIT; both are recorded in
`THIRD_PARTY_NOTICES.md`.

## D008: Require macOS 10.15 or newer

The bundled whisper.cpp/ggml runtime uses C++ filesystem APIs unavailable under the
older macOS 10.13 deployment target. Tauri is explicitly configured with
`bundle.macOS.minimumSystemVersion = 10.15`, which keeps release and CI builds aligned
and still supports Intel and Apple Silicon Macs from the intended usage range. The
repository Cargo configuration exports the same deployment target so CMake-based Rust
dependencies receive it before the application build script runs.

## D009: Keep network translation off the ASR thread

Committed Japanese text is submitted to a small bounded worker that calls an
OpenAI-compatible API with a fixed timeout. Slow or unavailable providers therefore do
not pause PCM ingestion or Whisper inference, and translation is disabled when no API
key is configured. Development builds read credentials from environment variables;
packaged settings must use the OS credential store rather than plain-text files.
