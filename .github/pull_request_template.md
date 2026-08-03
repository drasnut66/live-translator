## Summary

Describe the user-visible change and the affected platform(s).

## Checks

- [ ] `cargo fmt --manifest-path core/Cargo.toml -- --check`
- [ ] `cargo test --manifest-path core/Cargo.toml`
- [ ] `npm run build` in `desktop/`
- [ ] `cargo check --manifest-path desktop/src-tauri/Cargo.toml`

## Risk and licensing

Mention audio permissions, API handling, model changes, or upstream license impact.
