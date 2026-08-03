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
- First partial subtitle latency.
- Stable Japanese commit latency.
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

The offline ASR real-time factor target is met on the current Mac. Streaming latency,
translation latency, long-run memory, and recovery checks remain open until the live
pipeline is connected.
