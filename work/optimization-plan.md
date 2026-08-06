# ASR Optimization Plan

Date: 2026-08-05
Status: baseline applied; next items ready to implement

## TL;DR

- **Keep the current stack**: `large-v3-turbo-q5_0` + 8 s decode window (both already
  applied to the working tree and to the user machine's runtime settings).
- **Full-size large-v3 (q5_0/q8_0) is NOT worth it for live streaming**: ~2.5x slower
  per decode (avg 736-745 ms vs 291 ms), 3-4 s decode spikes that break real-time
  margins, and no clear accuracy gain on the fixture. Keep it as an offline-only option.
- **turbo q8_0 (834 MB)** runs at the same speed as turbo q5_0 and is now selectable in
  the UI, but measured quality is a wash on this fixture (it wins some segments, loses
  others). Optional.
- The remaining visible errors are mostly **segmentation and noise** problems (mid-sentence
  splits, laughter/BGM/overlap), not model-size problems. That is where the next wins are.

## Measurement matrix

180 s of `D:\myapp\1.mp4` through the real session loop (Silero VAD + rolling window +
stable-prefix coordinator + beam search, `no_context=true`, ja forced), RTX 3060 Ti CUDA,
decode window 8 s. Full transcripts in `D:\myapp\_asr_ab_evidence\`.

| model | size | committed segments | avg decode | max decode | realtime | verdict (live) |
|---|---|---|---|---|---|---|
| small q5_0 (old default) | 465 MB | 61 | 334 ms | 2065 ms | ~1.0x | usable, weakest |
| **large-v3-turbo q5_0** | 547 MB | 59 | 291 ms | 1202 ms | ~1.0x | **recommended** |
| large-v3-turbo q8_0 | 834 MB | 63 | 297 ms | 1689 ms | ~1.0x | optional; quality wash |
| large-v3 (full) q5_0 | 1031 MB | 57 | 736 ms | 3906 ms | ~1.0x avg, spikes | offline only |
| large-v3 (full) q8_0 | 1580 MB | 66 | 745 ms | 3889 ms | ~1.0x avg, spikes | offline only |

Notes:

- With a 4 s window (old default) the same runs commit only 40-44 segments and truncate
  long utterances mid-sentence; 8 s is the single biggest measured quality win
  (D022, `work/decisions.md`).
- Full-size large-v3 does capture slightly more content (66 vs 59 segments for q8_0) but
  introduces new errors (`青髪袋`, `黒色してるわ`, `春去る日`) and its 3.9 s decode max
  can stall the 1-decode-per-second cadence.
- Names (`ジョージョちゃん` / `嬢ちゃん` / `ジョージェちゃん`) and noisy overlap segments
  (`ミレチバンガード違う!ヘクロが`) are wrong in every model — segmentation/noise issue.

## Applied already (this session)

1. `desktop/src-tauri/src/session.rs`: decode window 4 s -> 8 s, override
   `LIVE_TRANSLATOR_DECODE_WINDOW_SECONDS` (clamped 2-10). `cargo check` passes.
2. User machine `asr-settings.json` -> `ggml-large-v3-turbo-q5_0.bin` (next session).
3. `desktop/src-tauri/src/asr/settings.rs`: `ggml-large-v3-turbo-q8_0.bin` added to
   `AVAILABLE_MODELS` (UI dropdown, no rebuild needed for model pick only if repackaged).
4. Models downloaded + verified: `ggml-large-v3-turbo-q8_0.bin` (834 MB),
   `ggml-large-v3-q5_0.bin` (1 GB), `ggml-large-v3.bin` (f16, 2.95 GB);
   `ggml-large-v3-q8_0.bin` (1.58 GB) quantized locally from f16 with whisper.cpp's
   `whisper-quantize` (official repo ships no full-size q8_0).
   Checksums recorded in `scripts/download_models.ps1` and
   `scripts/download_models.hfmirror.ps1`.
5. New A/B probe `desktop/src-tauri/examples/asr_ab_probe.rs` (configurable window via
   `LIVE_TRANSLATOR_DECODE_WINDOW_SECONDS`).

## Next items (prioritized)

### P1: Punctuation-aware segmentation (biggest remaining quality lever)

Problem: the session commits on ~300 ms of silence only, so fast talk-show speech is cut
mid-sentence ("プレゼントって" -> "自分では買わないようなもの?"). Whisper decodes short,
incomplete windows much worse than complete sentences.

Plan:

- In `session.rs`, when VAD signals `end_of_speech`, wait for a sentence boundary: either
  the committed text ends with Japanese sentence-final punctuation (`。！？…」`) or a longer
  silence (e.g. 700-900 ms) elapses, or the segment hits the 10 s cap.
- Keep the existing 1 s partial-decode cadence so subtitles still stream; only the *commit*
  is deferred to a boundary.
- Impact: fewer mid-sentence commits, better final decodes, fewer re-translations.
- Validate by re-running `asr_ab_probe` and comparing commit count + transcript quality.

### P1: Confidence gate for noisy segments

Problem: laughter/BGM/overlap segments still surface garbage (`(笑い声)`,
`ミレチバンガード違う!ヘクロが`). The current filter only drops segments with
`no_speech_probability > 0.6`.

Plan:

- In `whisper.rs` `collect_text_tokens`, also skip segments with low average log-probability
  (`segment.avg_logprob()`, exposed by whisper-rs) e.g. `< -1.0`, and/or lower
  `NO_SPEECH_THRESHOLD` to ~0.5.
- Tune the threshold with the evidence files (ground truth = what a human hears); keep
  real speech above the bar.

### P2: Display-level repetition suppression

D019 skips *re-translation* of repetition-run commits but the overlay still grows
`まだまだまだ…`. Add a token-level cap: if a commit's appended tokens are all already
committed (`is_repetition_run`), suppress the displayed growth too (keep the first
occurrence), so the subtitle row does not visually repeat.

### P2: Relax VAD split for talk-show pacing

`min_silence_duration = 300 ms` is aggressive. Try 500 ms (and/or raise `speech_pad`) to
group clauses; re-run the A/B to confirm no segment becomes too long for the 8 s window.

### P3: Temperature fallback for noisy windows

`temperature_inc = 0.0` means a single greedy beam pass. For windows with low confidence,
allow a second pass at higher temperature (e.g. 0.2-0.8) which sometimes reduces
hallucination on noisy audio; costs one extra decode per such window (CUDA headroom is OK).

### P3: Offline accuracy path

When realtime is not required (file transcription), use full-size large-v3 q5_0/q8_0 via a
non-realtime session or whisper-cli. Do not default to them for live.

## Model selection guide

| need | model | why |
|---|---|---|
| live streaming (default) | large-v3-turbo q5_0, window 8 s | best quality/latency balance, ~1.0x realtime |
| live, slightly higher fidelity | large-v3-turbo q8_0 | same speed; measured wash, but higher quant |
| offline batch accuracy | large-v3 (full) q5_0 or q8_0 | more content; not realtime-safe |
| low-RAM/old machines | small q5_0, window 8 s | still realtime on CPU+GPU; weakest accuracy |

## Rebuild

```powershell
powershell -ExecutionPolicy Bypass -File build_cuda.ps1 -Mode tauri
# copy target\release\live-translator-desktop.exe over LiveTranslator.exe, or install the MSI
```

Model selection in the overlay works without a rebuild (saved to `asr-settings.json`);
the 8 s window and the new dropdown entry need the repackage.

## Evidence

- `D:\myapp\_asr_ab_evidence\ab-small-w8.txt`, `ab-large-w8.txt` (turbo q5_0),
  `ab-turbo-q8-w8.txt`, `ab-full-q5-w8.txt`, `ab-largev3-q8-w8.txt`
- Probe: `desktop/src-tauri/examples/asr_ab_probe.rs`
- Quantize recipe (full-size q8_0): `C:\llm\whisper.cpp` (built `whisper-quantize`),
  `whisper-quantize ggml-large-v3.bin ggml-large-v3-q8_0.bin q8_0`
## Follow-up (same day): CUDA check, UI fixes, repackage

- **CUDA confirmed for all tested models.** Full-size large-v3 q8_0 ran with GPU util up
  to 74% and ~3.7 GB VRAM; the 2.5x decode slowdown vs turbo is the decoder architecture
  (32 layers vs 4), not a CPU fallback. turbo stays the live default; full-size stays
  offline-only.
- **Immediate model switching**: the overlay's ASR model dropdown is now enabled while a
  session runs; changing it saves the choice and restarts the running session with the new
  model automatically. No app restart needed.
- **Dropdown readability**: fixed white-on-white options in the WebView2 native listbox
  (`select option { color:#141414; background:#fff; }` in `desktop/src/App.css`).
- **Repackaged** after all changes: root `LiveTranslator.exe` refreshed; MSI + NSIS under
  `target\release\bundle\`. NSIS toolchain is cached under `%LOCALAPPDATA%\tauri\NSIS` so
  future `-Mode tauri` runs should skip the flaky github downloads (see handoff).
