## Summary

<!-- One or two sentences on what this change does and why. -->

## Related issue

<!-- e.g. Closes #123, Refs #456. Skip if not applicable. -->

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Refactor / cleanup
- [ ] Documentation
- [ ] Build / CI / tooling

## How I tested this

<!-- What you actually did to verify the change. If UI- or GPU-affecting, mention the platform(s) and GPU backend you ran it on. -->

## Project-specific checklist

<!-- Easy-to-miss conventions for TrueShow. See CONTRIBUTING.md for the full list. -->

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes with 0 warnings
- [ ] `cargo test --workspace` passes locally
- [ ] `pwsh -NoProfile -File scripts/check-coverage.ps1` maintains >= 90% Rust coverage
- [ ] `cd apps/desktop && npm run check && npm run test:coverage && npm run build` passes (>= 90% UI coverage)
- [ ] Live Audience / Stage / Musician outputs remain GPU-rendered in `trueshow-engine` (zero browser-rendered live outputs)
- [ ] Clean-room & license boundary respected (no bundled proprietary SDK DLLs, hardcoded API keys, or GPL-incompatible dependencies)
