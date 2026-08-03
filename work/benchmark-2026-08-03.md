# ASR Benchmark: 2026-08-03

## Input

- File: local `1.flv` fixture outside the public repository
- Actual container: MP4/MOV despite the `.flv` suffix
- Duration: 356.077574 seconds
- Video: H.264, 1280x720, 30 fps
- Audio: AAC LC, 44.1 kHz, stereo

## Runtime

- Machine: Apple M3
- Backend: whisper.cpp with Metal
- ASR model: `ggml-large-v3-turbo-q5_0.bin`
- VAD model: `ggml-silero-v6.2.0.bin`
- Language forced to Japanese
- Input conversion: FFmpeg to 16 kHz mono WAV

## Result

- Wall time: 40.43554 seconds
- Offline real-time factor: 0.1135
- Throughput: about 8.8 times faster than real time
- VAD speech segments: 69
- Text lines: 139
- SRT lines: 556

Outputs:

- `test-transcription.txt`
- `test-transcription.srt`
- `test-transcription.json`

The result demonstrates sufficient raw inference throughput for streaming on this Mac.

## In-process streaming probe

The first eight seconds were also decoded through `examples/live_asr_probe.rs` using a
single model context and state, a 1.5-second minimum window, one-second revision cadence,
and an eight-second rolling maximum. It produced seven incremental Japanese hypotheses.
The first input-timeline partial appeared at 1500 ms and the stable-prefix coordinator
finished with:

```text
はい、ということで2つ目のお題に行きたいと思います。いいですかジョージュさん?どうぞ!こちらになります。お題お願いします。
```

The debug run reported 5.941 seconds for its one-time embedded Metal library load.
Per-revision wall latency is not yet instrumented, so this result verifies the streaming
call path and model reuse rather than the sub-900 ms acceptance target.

## Streaming VAD probe

`examples/vad_probe.rs` processed the first 30 seconds through the same rolling Silero
adapter used by the Tauri session. It detected four endpoints at these input times:

```text
7.76 s
16.52 s
20.78 s
25.04 s
```

The run completed in about 1.1 seconds after compilation on the Apple M3. whisper.cpp
reported about 0.53 seconds of cumulative VAD inference, so this configuration leaves
ample headroom for concurrent ASR. Endpoint quality still needs a broader fixture set
covering background music, long silence, and very short utterances.

## Model checksums

```text
ggml-large-v3-turbo-q5_0.bin
394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2

ggml-silero-v6.2.0.bin
2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987
```
