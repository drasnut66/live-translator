# Desktop Overlay

Tauri 2, React, and TypeScript desktop shell for the Japanese-to-Chinese live translator.

## Commands

```sh
npm install
npm run build
npm run tauri dev
```

The Rust application reads model readiness from `../models/` and depends on the
backend-neutral streaming crate at `../core/`.

For development, download models from the repository root:

```sh
../scripts/download_models.sh
```

On Windows PowerShell, use `..\scripts\download_models.ps1`. Set
`LIVE_TRANSLATOR_MODEL_DIR` when models should live outside the checkout. Packaged
builds use the platform app-data directory unless that variable is set.

The current subtitle text is deterministic sample data from the local media smoke test.
Live audio and translation API wiring are the next implementation stage.

The release workflow builds macOS arm64, macOS x64, and Windows x64 bundles from this
directory. Signing and notarization are intentionally left to the maintainer's secrets.
