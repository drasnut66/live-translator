# Architecture

## Target pipeline

```text
audio source
  -> 16 kHz mono PCM
  -> 20 ms frame assembler
  -> bounded ring buffer
  -> Silero VAD
  -> whisper.cpp decode
  -> stable-prefix coordinator
  -> Japanese subtitle state
  -> translation scheduler
  -> provider API
  -> Japanese/Chinese overlay
```

## Boundaries

The application owns audio-source selection, buffering, VAD policy, stable-prefix
commit, subtitle revisions, API cancellation, caching, and UI state. whisper.cpp
owns model loading and inference only. WhisperLiveKit is a reference for LocalAgreement,
buffer trimming, and incremental result handling; it is not a runtime dependency in the
native build.

## Audio sources

1. Browser tab: Chrome extension captures tab audio and sends PCM to localhost.
2. Microphone: cross-platform capture through a Rust audio backend.
3. Windows system audio: WASAPI loopback adapter.
4. macOS system audio: ScreenCaptureKit adapter.
5. FFmpeg URL/file input for HLS, RTSP, and local media. The script remains the
   offline smoke-test path; `desktop/src-tauri/src/audio/ffmpeg.rs` is the incremental
   source used by the native runtime.

Every adapter emits the same `AudioFrame` contract. The rest of the application must
not branch on the operating system.

## ASR contract

```text
AsrPartial {
  segment_id,
  revision,
  start_ms,
  end_ms,
  stable_text_ja,
  unstable_text_ja
}

AsrCommit {
  segment_id,
  start_ms,
  end_ms,
  text_ja
}
```

Japanese uses token-prefix comparison, not whitespace comparison. Translation consumes
`AsrCommit`, never every raw partial hypothesis.

## Translation contract

```text
TranslationRequest {
  segment_id,
  revision,
  source_text,
  source_language,
  target_language,
  context,
  glossary
}
```

The current OpenAI-compatible provider runs in a separate bounded worker, supports a
request timeout, and emits a final result without blocking ASR. A later provider can
implement the same `TranslationJob`/`TranslationPayload` boundary. API keys are read
from environment variables during development and must move to the OS credential store
for packaged builds; they are never written to SQLite or JSON.

## Current implementation boundary

The incremental PCM contract, bounded queue, media decoder, long-lived in-process
whisper.cpp engine, Silero model, stable-prefix core, Tauri overlay, managed session, and
asynchronous translation worker have been verified. The ASR probe and native session
consume `FfmpegPcmSource::next_frame()` without relaunching a CLI process or reloading
model weights. The managed session now uses rolling Silero VAD inference to close
utterances after silence, with a bounded continuous-speech fallback. The next boundary
is microphone and system-audio capture.
