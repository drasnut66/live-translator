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

Providers must support cancellation, timeout, and a final result. Streaming output is
optional. API keys are stored in the OS credential store, not in SQLite or JSON.

## Current implementation boundary

The incremental PCM contract, bounded queue, media decoder, local whisper.cpp runtime,
Silero VAD, stable-prefix core, and Tauri overlay have been verified. The Chinese line
is still fixture data. No API key is read and no network translation request is made in
the current milestone. The next boundary is a long-lived whisper.cpp worker that
consumes `FfmpegPcmSource::next_frame()` without relaunching a CLI process per chunk.
