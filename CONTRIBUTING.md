# Contributing

## Before opening a pull request

Run the checks that match your change:

```sh
cd core
cargo fmt -- --check
cargo test

cd ../desktop
npm ci
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
```

Keep platform-specific code behind an explicit adapter or command boundary. Do not
commit model files, API keys, `node_modules`, `target`, generated bundles, or personal
media. Changes to subtitle stability should include a regression test in `core/`.

## Pull requests

Describe the user-visible behavior, affected platforms, test commands, and any model or
license implications. Small focused pull requests are easier to review and backport.
