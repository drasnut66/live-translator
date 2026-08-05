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

## D010: Reuse whisper.cpp's built-in Silero VAD runtime

The desktop session loads `ggml-silero-v6.2.0.bin` through the VAD API already exposed by
the pinned `whisper-rs` dependency. It evaluates a four-second rolling tail every 250 ms,
commits after approximately 500 ms of silence, and forces a boundary after 15 seconds of
continuous speech. This avoids a second ONNX runtime and keeps macOS/Windows packaging
identical.

## D011: Keep microphone callbacks lightweight and resample off the callback thread

CPAL owns cross-platform input-device access and delivers each device's native sample
format. The real-time callback only converts samples to `f32`, downmixes channels, and
pushes native-rate chunks into a five-second bounded queue. The consumer thread uses
Rubato's fixed-ratio FFT resampler to produce anti-aliased 16 kHz mono frames. This keeps
Whisper inference and resampling work off the audio callback while preserving one
`AudioFrameSource` contract for media and microphone sessions.

## D012: Raise the macOS baseline to 13 for in-process system-audio capture

This supersedes D008's 10.15 baseline. ScreenCaptureKit system-audio capture is available on
macOS 13 or newer, and the selected Rust binding compiles its bridge with that minimum target.
Keeping the app at 10.15 would create a package that links against unavailable framework
symbols. The Tauri bundle setting and Cargo deployment target both use 13.0; Windows remains
unaffected. The macOS adapter captures a selected display, excludes this application's audio,
and requests 48 kHz mono float PCM before handing it to the common bounded frame path.

## D013: Store translation keys in the platform credential store

The application writes OpenAI-compatible API keys to macOS Keychain or Windows Credential
Manager through the `keyring` crate. The IPC settings response never contains the key; it only
reports whether a credential exists. Endpoint and model are non-sensitive settings in the app
data directory, while development environment variables stay as a fallback for unattended
local runs. This removes a packaged-build dependency on plain-text secrets without breaking
the existing developer workflow.

## D014: Keep DeepL behind the existing translation worker

DeepL uses the same committed-subtitle queue and timeout as the OpenAI-compatible client. Its
adapter sends one Japanese text per `POST /v2/translate` JSON request with the
`DeepL-Auth-Key` authorization header and targets Simplified Chinese. Provider selection is
persisted separately from the credential account, so switching providers never sends one
provider's key to another API.

## D015: Keep cloud translation and skip a local NLLB runtime

A local NLLB/CTranslate2 fallback was evaluated and explicitly rejected for the initial
product. Adding it would bundle a second inference runtime plus a ~600 MB distilled NLLB
model on top of the existing whisper.cpp + Silero assets, roughly doubling the distributed
model size and adding CPU translation latency of seconds per sentence. Personal subtitle
volume stays far inside the free tier of DeepL (500k characters/month) or a low-cost
OpenAI-compatible endpoint, which already run off the ASR thread with a bounded queue and
timeout. This keeps the app lightweight and adds zero model-distribution surface.

**Why:** the user prioritizes a lightweight build; the free cloud tier covers expected usage.
**How to apply:** do not schedule NLLB/CTranslate2 integration for the current milestone.
Revisit only if offline translation or fixed-price volume becomes a hard requirement, at
which point the ONNX Runtime path (the only mature local option with a Rust binding) would be
assessed against that requirement.

## D016: Decode each Whisper window with no_context on

`set_no_context(false)` — the whisper.cpp default — feeds the previous decode window's
tokens back as prompt context. On low-information or overlapping audio the large model
snowballs that tail into repetition: measured on large-v3-turbo, a single segment grew
`"ごめんなさい"` indefinitely, re-translating every second. `set_no_context(true)` makes
each window independent: repetition gone, and decode got ~2.8× faster (max 1338 → 474 ms)
because there is no context to re-process. Cross-window hypothesis stability is slightly
reduced, which is why the stable-prefix coordinator exists. `LIVE_TRANSLATOR_NO_CONTEXT=0`
re-enables context for experimentation.

**Why:** eliminates the dominant "subtitle stuck repeating one phrase" failure mode.
**How to apply:** keep no_context on by default; do not reintroduce cross-window context
without re-measuring the repetition failure on the large model.

## D017: Translation runs on a CUDA llama.cpp server (Hy-MT)

The originally bundled `hymt/llama-server.exe` was CPU-only (no ggml-cuda.dll), so
`-ngl` was ignored and each Japanese→Chinese segment took ~2100 ms on CPU. A CUDA build
of llama.cpp was compiled in place (`C:\llm\llama.cpp`, `LLAMA_CUDA=ON`,
`CMAKE_CUDA_ARCHITECTURES=86`) and deployed to `models/hymt/`; `-ngl 99` puts all layers
on the RTX 3060 Ti. Per-translation latency dropped to ~125-190 ms. The current llama.cpp
supports Hy-MT's `hunyuan-dense` architecture (`src/models/hunyuan-dense.cpp`), which the
old bundled server predates. The CPU edition is preserved at `models/hymt/cpu-backup/`.
`--ctx-size 4096` and q8_0 KV cap llama-server RAM at ~1.9 GB (was 5-8 GB).

**Why:** offline translation must keep up with live subtitles; CUDA is available and
cheap on this machine.
**How to apply:** the app treats the Hy-MT model directory as the llama.cpp home (server +
DLLs live there). Re-deploying llama.cpp means copying the exe and `build-cuda/bin/*.dll`
into `models/hymt/`; CPU fallback files remain in `cpu-backup/`.

## D018: A distributable exe requires `tauri build`, not `cargo build`

`cargo build --release` compiles only the Rust backend; the Tauri exe then resolves the
frontend at the Vite dev URL (`localhost:1420`) and shows "connection refused" without the
dev server. `npm run tauri build` runs the `beforeBuildCommand` (`npm run build`) and
bundles `desktop/dist/` into the binary, plus MSI/NSIS packages. `build_cuda.ps1 -Mode
tauri` is the single command for a distributable CUDA build.

**Why:** confusing runtime failure when a release exe was produced with cargo alone.
**How to apply:** always use `build_cuda.ps1 -Mode tauri` for anything the user will run
outside the dev server; `-Mode release` is only for compile checks.

## D019: Skip re-translation for repetitive-append commits

`StablePrefixCoordinator` commits each newly-agreed token, so a segment whose hypothesis
grows from "舌打ち!" to "舌打ち!舌打ち!" to "舌打ち!舌打ち!舌打ち!" emits a commit per
step, and each commit used to re-translate — flooding the subtitle row. `AsrCommit` now
carries `translation_skippable`, true when the appended tokens are all already committed
(`is_repetition_run`). `session.rs` still emits the frontend event (so the displayed
Japanese grows) but skips the translation worker; the frontend keeps the last real
translation instead of resetting to "等待翻译". Meaningful appends ("今日は" → "今日は晴れ")
are never flagged.

**Why:** a single noisy/repetitive segment should not re-translate every second.
**How to apply:** keep the flag computed from token ids, not text; the first commit of any
segment is always translated.

## D020: Gate the CUDA feature behind a non-default cargo feature

whisper-rs-sys's `cuda` feature hard-requires `CUDA_PATH` in its build script
(`env::var("CUDA_PATH").unwrap()`), which does not exist on GitHub's windows-latest CI
runner — a plain push of `features = ["cuda"]` would break CI. The desktop crate now
defines a package-level `cuda = ["whisper-rs/cuda"]` feature, off by default, and
`build_cuda.ps1` passes `--features cuda` for local release/tauri builds. CI compiles the
default (CPU) build, which needs no CUDA toolchain. `default_use_gpu()` is
`cfg!(all(windows, feature = "cuda"))`, so the CPU CI build defaults to CPU inference.

**Why:** keep the repo's CI green while retaining local CUDA acceleration.
**How to apply:** never put `cuda` in `default`. Any code that behaves differently with the
GPU backend must key off `cfg!(feature = "cuda")`, not `cfg!(target_os)`.

## D021: Local CUDA builds require the nightly toolchain

Rust stable 1.97.1 panics inside rustc (`TryFromIntError(PosOverflow)`) when compiling the
cuda feature path of this crate; nightly 1.99.0 compiles it. `desktop/src-tauri/
rust-toolchain.toml` pins nightly for local builds and is git-ignored so CI (stable) stays
on stable. CI never compiles the cuda feature, so stable is safe there.

**Why:** stable rustc is buggy on this specific feature path; nightly is the working
toolchain.
**How to apply:** keep `rust-toolchain.toml` local-only (git-ignored). If a future stable
rustc fixes the panic, the pin can be dropped.
