# Contributing to TrueShow

Thanks for your interest in contributing to **TrueShow**!

## Architecture Rules

1. **Zero Browser-Rendered Live Outputs:** Physical Audience, Stage, and Musician screens as well as broadcast feeds (NDI/OMT/DeckLink/Syphon/Spout) composite directly on the GPU inside `trueshow-engine` (`crates/render-wgpu` + `bins/engine`).
2. **Sanctuary Zero-Blackout Resilience:** Reloading, crashing, or restarting the Tauri 2 operator UI (`apps/desktop`) must never interrupt physical sanctuary displays.
3. **Clean-Room Licensing:** Core crates are licensed under **MPL-2.0**; `crates/plugin-sdk` and `wit/` are dual-licensed under **MIT OR Apache-2.0**. Never commit proprietary SDK binaries (NDI, DeckLink, LGPL FFmpeg DLLs) or hardcoded third-party API keys.

## Local Verification Checklist

Before opening a pull request, run the full lint, test, and >= 90% coverage suite:

```powershell
# Rust formatting, Clippy, and workspace tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings

# Rust LLVM line/region coverage (>= 90% gate)
pwsh -NoProfile -File scripts/check-coverage.ps1

# Svelte 5 + Tauri 2 UI checks & coverage (>= 90% gate)
cd apps/desktop
npm run check
npm run test:coverage
npm run build
```

## Pull Requests & Issues

- Use the provided issue templates for bug reports and feature requests.
- Keep PRs focused and include tests for both happy paths and failure paths.
