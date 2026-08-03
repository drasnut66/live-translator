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
2. Microphone: CPAL captures the selected native input format, downmixes it to mono, and
   uses Rubato FFT resampling before emitting 16 kHz frames.
3. Windows system audio: WASAPI render-loopback adapter, converted from 48 kHz stereo float
   PCM to mono before entering the bounded native queue.
4. macOS system audio: ScreenCaptureKit adapter for a selected display on macOS 13 or newer.
   It requests Screen Recording permission, excludes the application's output to prevent
   feedback, requests 48 kHz mono float PCM, and enters the same bounded native queue.
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

OpenAI-compatible and DeepL providers run in a separate bounded worker, support a request
timeout, and emit a final result without blocking ASR. A later provider can implement the
same `TranslationJob`/`TranslationPayload` boundary. The Tauri settings
commands keep API keys in macOS Keychain or Windows Credential Manager and return only
configuration state to the webview. Endpoint and model are stored separately in
`translation-settings.json` under the application data directory; that file has no API-key
field. Development environment variables remain a fallback until a credential is saved.

## Current implementation boundary

The incremental PCM contract, bounded queues, media, microphone, and system-audio sources,
long-lived in-process whisper.cpp engine, Silero model, stable-prefix core, Tauri overlay,
managed session, and asynchronous translation worker are connected. Every source implements
`AudioFrameSource`; the session does not branch after source construction. The managed session
uses rolling Silero VAD inference to close utterances after silence, with a bounded
continuous-speech fallback. Browser-tab capture is the remaining audio-source boundary.
