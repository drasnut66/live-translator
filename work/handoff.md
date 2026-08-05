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
- Measured streaming model performance with `examples/soak_probe.rs` on Apple M3
  (Metal): large-v3-turbo runs at 0.49x realtime, whisper-small and whisper-base both at
  1.00x. base's Japanese accuracy is noticeably worse, so whisper-small is now the
  default runtime model with large-v3-turbo selectable via
  `LIVE_TRANSLATOR_WHISPER_MODEL`. Memory stayed flat in all runs. See
  `benchmark-2026-08-03.md`.
- Added browser-tab capture: a Chrome MV3 extension (`browser-tab-capture/`) captures the
  active tab's audio with `tabCapture` and streams 16 kHz mono f32 over a localhost
  WebSocket, and a `BrowserTabPcmSource` (`audio/websocket.rs`) feeds it through the shared
  `BufferedPcmCapture` frame path. Verified end to end with
  `examples/browser_tab_probe.rs` and `scripts/websocket_client_sim.py`: a sine fixture
  arrived with `peak=0.6000`, zero dropped samples, and reconnect handling.

## Environment status

- CMake and Apple Clang are available.
- Node.js and npm are available.
- Rust 1.97.1 and Cargo 1.97.1 are available.
- FFmpeg 8.1.2 is available.
- whisper.cpp was built with Metal in the local development checkout.
- `ggml-small.bin` (default), `ggml-large-v3-turbo-q5_0.bin` (optional), and
  `ggml-silero-v6.2.0.bin` are present.
- Hy-MT1.5-1.8B Q4_K_M GGUF plus a bundled llama-server binary are present under
  `models/hymt/` for the offline "local" translation provider.
- Core tests, the production web build, and the Tauri Rust check pass. macOS system-audio
  capture is verified non-silent; Windows WASAPI hardware validation remains pending.

## Next executable tasks

1. Run the equivalent WASAPI hardware probe on a Windows host.
2. Evaluate ASR quality tuning (beam search + initial prompt on whisper-small) to improve
   Japanese recognition, which the user reports as weak.
3. Tag and release v0.1.0; the core feature set is complete.

## Current blockers

There is no active toolchain blocker. The media-input pipeline, managed session,
VAD-driven segmentation, Tauri events, optional API translation, microphone capture,
macOS system-audio capture, and browser-tab capture (via a Chrome MV3 extension and a
localhost WebSocket source) are connected and verified. Windows system-audio hardware
validation is pending a Windows host. The `.flv` extension is misleading; FFmpeg
identifies `1.flv` as MP4/MOV, so all media paths must continue to probe content rather
than trust the suffix.

The macOS transparent Tauri window enables `macOSPrivateApi`; this is acceptable for
personal side-loading but incompatible with Mac App Store submission.

---

## Windows migration + CUDA acceleration (2026-08-04)

Migrated the project from macOS (Apple M3) to Windows 11 (AMD Ryzen 7 3800X, RTX 3060 Ti)
and enabled CUDA-accelerated Whisper.

### Code changes (uncommitted, working tree)

- `desktop/src-tauri/Cargo.toml`: on Windows, `whisper-rs = { version = "=0.16.0",
  features = ["cuda"] }`. `whisper-rs` for non-macOS/non-Windows stays CPU-only.
- `desktop/src-tauri/src/asr/whisper.rs`: `use_gpu` defaults on for macOS (Metal) and
  Windows (CUDA). New `LIVE_TRANSLATOR_FORCE_CPU=1` env var forces CPU even when a GPU
  backend exists.
- `desktop/package.json`: added `allowScripts.esbuild@0.28.1: true` so npm ci installs
  esbuild's binary (npm 11 blocks postinstall scripts by default).
- Untracked: `desktop/src-tauri/rust-toolchain.toml` (pins nightly channel),
  `scripts/download_models.hfmirror.ps1` (hf-mirror.com variant of the model downloader
  for the China network).

### Toolchain installed on this Windows machine

- Rust 1.97.1 stable + MSVC via rustup (`~/.cargo/bin`).
- FFmpeg 8.1.2 (winget `Gyan.FFmpeg`), bin dir added to user PATH.
- CUDA Toolkit 13.1 at `D:\Environment\CUDA\v13.1` (custom path, installed without VS
  integration). nvcc 13.1.80. Machine `CUDA_PATH` points there.
- CUDA 12.6 remains at `D:\Environment\CUDA Toolkit v12.6` (unused; its nvcc cannot
  drive MSVC 14.51).
- MSVC 14.44 toolset (v143) is present inside `D:\Microsoft Visual Studio\18\Community`
  alongside 14.51. **CUDA compilation must use `-vcvars_ver=14.44`** — nvcc 13.x
  rejects MSVC 14.51 (cudafe++ crashes with ACCESS_VIOLATION). GCC/MinGW and Clang
  cannot serve as nvcc host compilers on Windows.
- whisper.cpp (bundled via whisper-rs-sys 0.15.0) built with `GGML_CUDA=ON`; the
  ggml-cuda backend is statically linked. sm_86 (3060 Ti) is auto-detected.

### Critical build constraint: ASCII path only

whisper-rs-sys's CMake 4.3 CUDA compiler-id probe fails when the build directory path
contains non-ASCII characters (it errors on `tmp/CMakeCUDACompilerId.cpp4.ii`). The
project path `D:\同传` breaks CUDA builds. The build must run with
`CARGO_TARGET_DIR` pointing to a pure-ASCII path.

- Working build dir used during the session: `C:\wpt\target` (8 GB of release+debug
  artifacts including a working CUDA whisper build).
- The `D:\同传\target` and `D:\myapp\target` dirs are almost empty (6.5 MB) — not the
  real build cache.

### Build + run recipe (Windows)

```powershell
$vcvars = 'D:\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat'
cmd /c "call `"$vcvars`" -vcvars_ver=14.44 >nul 2>&1 && set"  # then load env vars
$env:CARGO_TARGET_DIR = 'C:\wpt\target'          # ASCII path, required
$env:CUDA_PATH = 'D:\Environment\CUDA\v13.1'
$env:CUDA_HOME = $env:CUDA_PATH
$env:CUDACXX = 'D:\Environment\CUDA\v13.1\bin\nvcc.exe'
$env:CMAKE_GENERATOR = 'Ninja'
$env:PATH = "$env:CUDA_PATH\bin\x64;$env:CUDA_PATH\bin;...\ninja;...\ffmpeg\bin;" + $env:PATH
cd D:\同传\live-translator\desktop\src-tauri
cargo build --release   # or `cargo run --example live_asr_probe -- <model> <video> <sec>`
```

Runtime DLL note: CUDA 13.1's runtime DLLs (cudart64_13.dll, cublas64_13.dll, etc.)
live under `D:\Environment\CUDA\v13.1\bin\x64` (not `bin`). The PATH must include
`bin\x64` or the exe fails with `STATUS_DLL_NOT_FOUND`.

`tauri build` (via `npm run tauri build`) produces a standalone custom-protocol exe
`target/release/live-translator-desktop.exe` that does not need the Vite dev server.
A copy lives at the repo root as `LiveTranslator.exe`.

### Performance (30 s of 1.mp4, live_asr_probe)

| model | CPU (before) | CUDA (after) |
|---|---|---|
| ggml-small | real-time, ~846 MB RAM | real-time, 26% GPU util |
| large-v3-turbo | lagged badly (~12.5s behind) | real-time, keeps up with audio |

GPU VRAM usage ~1.5 GB. The large-v3-turbo model is now usable for live streaming,
fixing the original high-latency complaint.

### Models present under `models/`

- `ggml-small.bin` (default runtime model)
- `ggml-large-v3-turbo-q5_0.bin`
- `ggml-silero-v6.2.0.bin` (VAD, required)
- `speakrs/` (diarization ONNX models)
- `hymt/HY-MT1.5-1.8B-Q4_K_M.gguf` + `hymt/llama-server.exe` (+ the ggml/llama DLLs
  needed to run it)

### Env var notes

- `LIVE_TRANSLATOR_MODEL_DIR` was set (user-level) to the models directory so the
  packaged exe finds models without a terminal. The app also looks for `models/` next
  to the exe and under the app-data dir.
- `LIVE_TRANSLATOR_FORCE_CPU=1` forces CPU if needed.

### Build cache migrated to D:\myapp\target (2026-08-04)

- The 8 GB `C:\wpt\target` cache was copied to `D:\myapp\target` and `C:\wpt` was
  deleted. File count verified (12084), release exe + MSI/NSIS bundles intact.
- Since `D:\myapp` is a pure-ASCII path, `CARGO_TARGET_DIR=D:\myapp\target` works
  directly (no more `C:\wpt` workaround). `build_cuda.ps1` at the repo root encodes
  the full build recipe (vcvars 14.44 + CUDA env + target dir).
- tauri-build's generated permission files embed absolute `$CARGO_TARGET_DIR` paths.
  After a cache move, `cargo build --release` may fail with
  `failed to read plugin permissions: ... C:\wpt\...`; fix is to `rm -rf
  target/release/build/tauri` and rebuild (tauri crates rebuild in ~8 min; the CUDA
  whisper/ggml libs do not need recompiling if their build dir is untouched).
- Release build verified after migration: `cargo build --release` succeeds, GPU loads
  (VRAM ~2 GB, app RAM 172 MB). `LIVE_TRANSLATOR_MODEL_DIR` user env now points to
  `D:\myapp\live-translator\models`.
- The migrated exe is at `D:\myapp\target\release\live-translator-desktop.exe`; the
  repo-root `LiveTranslator.exe` copy was refreshed from it.

### Remaining

- Verify the full overlay session (not just the probe) under CUDA at the new location.

---

## Subtitle repetition, llama-server memory, and GPU translation (2026-08-05)

Follow-up fixes measured with the real app session loop (VAD + rolling window +
coordinator + local translation) on `1.mp4`.

### 1. llama-server memory bloat — fixed

`local_mt.rs` spawned llama-server **without `--ctx-size`**, so it defaulted to the
model's full context and the KV cache ate 3-6 GB. Added:

```
-ngl 0 → 99        # (later CUDA build; see §4) load all layers onto GPU
-t 2 → 4           # Ryzen 7 3800X 8c/16t
--ctx-size 4096    # 128 max output tokens; KV cache ~300 MB instead of GBs
--cache-type-k q8_0 / --cache-type-v q8_0
```

CPU edition measured: **1982 MB** working set (was 5-8 GB). Translations still correct.

### 2. "Stuck / repeating same word" — root cause found

NOT a full window. Real cause: **`set_no_context(false)` fed the previous window's
tokens back as prompt context**, so a hallucinated/overlapping tail snowballed into
repetition on the large model. Measured on `large-v3-turbo-q5_0` over 50 s of `1.mp4`:

```
no_context=0:  seg=6 "おーっとごめんなさい"
               seg=6 "おーっとごめんなさいがなさすぎてごめんなさいごめんなさい"
               seg=6 "...ごめんなさいめんなさいさん誰のプレゼントですかこれは私"   ← indefinite growth
               max decode 1338 ms, avg 393 ms, 9 translations (seg=6 re-translated 4×)
no_context=1:  seg=6 "あと髪の頃" → "...めんね気にせずでじゃあ次の木さん誰のプレゼントですかこれは"
               natural growth, NO repetition, max decode 474 ms, avg 267 ms
```

Fix: `whisper.rs` now sets `set_no_context(true)` by default. `LIVE_TRANSLATOR_NO_CONTEXT=0`
restores cross-window context. This also made decode ~2.8× faster on large (no context
baggage), so the 10 s window-forced segment boundary no longer lags.

### 3. Repeated-append commits skip re-translation (core dedup)

`core/src/lib.rs` `AsrCommit` gained `translation_skippable: bool`, computed by
`is_repetition_run(appended, committed)`: true when the new committed tokens are all
already present (e.g. "舌打ち!" → "舌打ち!舌打ち!"). The first commit of a segment is
always translated. `session.rs` `emit_commit` still emits the frontend event but skips
`translation.submit()` when flagged; `App.tsx` keeps the last real translation instead of
resetting to "等待翻译". This prevents one noisy segment from re-translating every second.

### 4. Translation moved to CUDA (llama.cpp)

The bundled `hymt/llama-server.exe` was a **CPU-only build** (no ggml-cuda.dll), so
`-ngl` was a no-op and each translation took ~2100 ms on CPU. Built a CUDA-enabled
llama.cpp from source:

```
git clone https://github.com/ggml-org/llama.cpp.git  →  C:\llm\llama.cpp
cmake -S . -B build-cuda -G Ninja -DLLAMA_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=86 \
      -DCMAKE_BUILD_TYPE=Release -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_SERVER=ON
cmake --build build-cuda --target llama-server
```

- Hunyuan-dense architecture (Hy-MT's GGUF) IS supported in current llama.cpp
  (`src/models/hunyuan-dense.cpp`); the old bundled server was just too old.
- `C:\llm\out\` holds `llama-server-cuda.exe` + `ggml-cuda.dll` (50 MB) + the other
  runtime DLLs from `build-cuda\bin\`. Deployed to `models/hymt/`; the CPU edition is
  backed up under `models/hymt/cpu-backup/`.
- `local_mt.rs` uses `-ngl 99` (all layers on GPU).
- Measured: **125-190 ms per translation** (was ~2100 ms) — ~12× faster. VRAM ~2.4 GB.

### 5. Frontend must be bundled (tauri build vs cargo build)

`cargo build --release` alone produces an exe that points at the Vite dev URL
(`localhost:1420` → "connection refused"). A distributable exe MUST be built with
`npm run tauri build` (the `beforeBuildCommand` runs `npm run build`, bundling
`desktop/dist/` into the binary). `build_cuda.ps1 -Mode tauri` does this and also
produces the MSI + NSIS bundles.

### Current code changes (uncommitted, working tree)

- `core/src/lib.rs` — `AsrCommit.translation_skippable`, `is_repetition_run`, tests.
- `desktop/src-tauri/src/session.rs` — `SubtitleCommitPayload.translation_skippable`,
  gate `translation.submit()` on it.
- `desktop/src/App.tsx` — keep last real translation on skippable commits.
- `desktop/src-tauri/src/asr/whisper.rs` — removed `set_initial_prompt`
  (`"これは日本語の字幕です。"` was echoed as `私は日本語の字幕です。` on low-confidence
  audio); `set_no_context(true)` default.
- `desktop/src-tauri/src/local_mt.rs` — `--ctx-size 4096`, `-t 4`, KV q8_0, `-ngl 99`.
- Untracked: `build_cuda.ps1` (repo root), `LiveTranslator.exe` (built artifact).

### Build recipe (Windows, current)

```powershell
powershell -ExecutionPolicy Bypass -File build_cuda.ps1 -Mode release  # cargo build --release --features cuda
powershell -ExecutionPolicy Bypass -File build_cuda.ps1 -Mode tauri    # npm run tauri build -- --features cuda
```

Runtime env: `LIVE_TRANSLATOR_MODEL_DIR=D:\myapp\live-translator\models`;
PATH must include `D:\Environment\CUDA\v13.1\bin\x64`. For the large model set
`LIVE_TRANSLATOR_WHISPER_MODEL=ggml-large-v3-turbo-q5_0.bin`.

### CI compatibility + feature gating (2026-08-05)

To keep GitHub CI green while still building CUDA locally, the `cuda` feature is gated:

- `desktop/src-tauri/Cargo.toml` defines a package-level feature:
  `cuda = ["whisper-rs/cuda"]`, **not** in `default`. The Windows dependency is
  `whisper-rs = { version = "=0.16.0" }` (no hardcoded feature).
- `build_cuda.ps1 -Mode release/tauri` pass `--features cuda`; `-Mode check` does not.
- `whisper.rs::default_use_gpu` is now `cfg!(all(windows, feature = "cuda"))`, so a
  CPU-only CI build defaults `use_gpu=false` (whisper.cpp would silently fall back to
  CPU anyway if it stayed true).
- Verified: `cargo check` with no `CUDA_PATH` and no feature compiles clean
  (this is what GitHub CI windows-latest runs). Local `--features cuda` builds the
  working CUDA exe.

**Nightly toolchain is REQUIRED for local CUDA builds.** `stable-x86_64-pc-windows-msvc`
1.97.1 panics inside rustc itself (`TryFromIntError(PosOverflow)`, "compiler unexpectedly
panicked") when compiling the `cuda` feature path. `nightly` 1.99.0 compiles it fine.
`desktop/src-tauri/rust-toolchain.toml` pins nightly but is **git-ignored**, so CI
(which pins stable) is unaffected. CI never builds the cuda feature, so stable is safe there.

`.gitignore` now also excludes the built `LiveTranslator.exe` artifact.

### What to push to GitHub

Source/config only. Tracked changes ready to commit:

- `core/src/lib.rs`, `desktop/src-tauri/src/{session,whisper,local_mt}.rs`,
  `desktop/src/App.tsx`, `desktop/src-tauri/Cargo.toml`, `desktop/package.json`
- `work/handoff.md`, `work/decisions.md`, `.gitignore`
- Untracked (intended): `build_cuda.ps1`, `scripts/download_models.hfmirror.ps1`

Excluded (git-ignored): `LiveTranslator.exe`, `desktop/src-tauri/rust-toolchain.toml`,
`models/`, `target/`, `node_modules/`, `dist/`.
