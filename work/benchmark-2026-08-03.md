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
It does not yet measure first-partial or stable-commit latency because the current media
runner performs a batch CLI invocation. Those measurements belong to the long-lived
streaming worker milestone.

## Model checksums

```text
ggml-large-v3-turbo-q5_0.bin
394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2

ggml-silero-v6.2.0.bin
2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987
```
