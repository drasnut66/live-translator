# Test Plan

## Media smoke test

Input: a local Japanese media fixture outside the public repository.

Checks:

- FFmpeg can probe the container and find an audio stream.
- The decoder produces 16 kHz, mono, signed PCM.
- The ASR process starts once and keeps the model warm.
- Japanese text is emitted without translating it to English.
- A short silence causes a commit rather than duplicate segments.

Status: completed for the full 356.08-second test file. See
`benchmark-2026-08-03.md` and the generated `test-transcription.*` artifacts.

## Streaming checks

- Arbitrary FFmpeg stdout read boundaries produce exact 20 ms, 16 kHz mono frames.
- A final partial frame flushes complete samples and ignores a truncated byte.
- A full audio queue drops the oldest frame and increments its drop counter.
- Repeated in-process Japanese partials and a final stable-prefix commit. Functional
  path passed on the first eight seconds of `1.flv`; wall latency remains to instrument.
- Rolling Silero VAD endpoint detection. The first 30 seconds of `1.flv` produced four
  endpoint commits at 7.76, 16.52, 20.78, and 25.04 seconds.
- Native microphone enumeration and capture. A two-second MacBook Air microphone probe
  emitted 2000 ms of 16 kHz mono PCM with peak 0.0185 and zero dropped native samples.
- Microphone conversion tests cover stereo downmix, 20 ms framing, same-rate passthrough,
  anti-aliased 48-to-16 kHz conversion, and five-second queue overflow behavior.
- System-audio adapters use the same bounded native capture queue and 16 kHz mono frame
  contract. The macOS path requires a Screen Recording permission grant and an audible output
  fixture; the current Mac probe captured 5000 ms with zero dropped samples while a 20-second
  fixture played, but reported `peak=0.0000`. Non-silent macOS output still requires a direct
  grant to the executable under test; Windows hardware validation remains required.
- Translation settings validate HTTP(S) provider endpoints, normalize persisted values, and
  verify that the serialized app-data settings contain no API-key field. Desktop tests do not
  write to the user's credential store.
- DeepL response parsing trims a successful translated text and rejects unsuccessful responses
  without exposing the provider response body.
- First partial subtitle wall latency.
- Stable Japanese commit wall latency.
- Chinese translation latency.
- Revision count per segment.
- Audio queue depth and dropped stale partial jobs.
- Real-time factor and peak memory.

## Required fixtures

- Clean Japanese speech.
- Japanese speech with background music.
- Fast speech with sentence-final particles.
- Long silence followed by a new sentence.
- A segment containing names, numbers, and mixed English.

## Acceptance target

On the target machine, a 30-minute test stream must not grow memory continuously,
must recover from a temporary translation API failure, and must keep the audio queue
bounded. Initial targets are less than 900 ms for the first partial Japanese result,
less than 1.8 s for a stable Japanese result, and less than 3 s for confirmed Chinese.

## Streaming soak result (2026-08-03)

`examples/soak_probe.rs` replayed the full 356-second `1.flv` through the realtime
managed-session loop (FFmpeg `-re` source, rolling Silero VAD, whisper.cpp large-v3-turbo
with Metal, stable-prefix coordinator).

- Memory was bounded and flat: 45 RSS samples over ~12 minutes wall time ranged
  724–791 MB and ended 40 MB below the start; no continuous growth.
- The pipeline could not keep real time: realtime factor 0.49x, average decode
  2136 ms, first partial at 1500 ms input time (target <900 ms). 356 s of input took
  about 733 s of wall time.
- The 0.49x bottleneck is single-window decode latency with large-v3-turbo. The
  batch benchmark's 8.8x factor does not transfer to the streaming cadence, where a
  ~1–2 s window is decoded every second. Streaming real time likely requires a
  smaller model (whisper-small/base) or a longer decode cadence; this is now a
  data-driven model-selection decision.

Streaming wall latency, translation latency, API recovery, Windows microphone hardware,
and system-audio checks remain open.
