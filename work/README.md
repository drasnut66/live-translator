# Work Notes

This directory is the handoff and engineering-notes area for the personal live translator.

## Documents

- `architecture.md`: component boundaries and streaming contracts.
- `decisions.md`: decisions that should not be silently reversed.
- `handoff.md`: current status, blockers, and the next executable tasks.
- `test-plan.md`: test cases and performance measurements.
- `benchmark-2026-08-03.md`: measured ASR result for the bundled test media.

## Repository layout

- `../core/`: backend-neutral streaming core.
- `../desktop/`: Tauri desktop shell.
- `../scripts/`: model and media helpers.
- `../models/`: ignored local model directory.

The reference checkouts and the local test video are intentionally outside the public
repository. See the root README for the upstream project links and setup boundary.

Update these notes when an implementation decision changes the public interfaces or the platform behavior.
