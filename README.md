# OpenPresenter

Clean-room, open-source, cross-platform (**Windows, macOS, Linux**) worship presentation engine built in **Rust (`wgpu` 30 + `winit` 30)** with a **Tauri 2 + Svelte 5** operator control surface.

## User Guide

New to OpenPresenter? Read the **[Complete OpenPresenter User Guide (with Screenshots)](docs/USER_GUIDE.md)** for a full walkthrough of every workspace, menu, right-click menu, drag-and-drop workflow, song arrangement, cue recorder, importer, CCLI attribution setting, broadcast output, and keyboard shortcut.

## Architecture Principles

- **Zero Browser-Rendered Live Outputs:** Physical Audience displays, Stage displays, and broadcast feeds (NDI/OMT/SDI/Syphon/Spout) composite directly on the GPU inside a decoupled sidecar process (`openpresenter-engine`).
- **Sanctuary Zero-Blackout Resilience:** Killing or reloading the Tauri 2 operator UI never drops physical sanctuary screens to black; the UI re-attaches seamlessly to the running engine.
- **Single Source of Truth:** Operator live previews and the WYSIWYG Slide Editor stream native GPU-rendered RGBA8 frames over lock-free shared memory and Tauri 2 raw binary `Channel<Response>` payloads.
- **Clean-Room & Copyleft:** Core crates are licensed under **MPL-2.0**; `crates/plugin-sdk` and `wit/` are dual-licensed under **MIT OR Apache-2.0**. Dynamic NDI 6, DeckLink, and LGPL-2.1+ FFmpeg libraries are never bundled in the repository.

## Completed Milestones (M0 – M3)

| Milestone | Scope Delivered | Key Crates |
| :--- | :--- | :--- |
| **M0** | Dual-window `wgpu` 30 compositor, decoupled `openpresenter-engine` sidecar IPC + lock-free SHM preview ring, Tauri 2 + Svelte 5 PoC | `core-scene`, `core-ipc`, `render-wgpu`, `bins/engine`, `apps/desktop` |
| **M1** | 7-layer GPU compositor (`Dissolve`/`PushLeft`/`WipeRight`, masks, Looks), cached broadcast typography rasterizer (`Vector2DRasterizer`), offline SQLite WAL + FTS5 library, seamless looping media engine & lock-free planar `cpal`/`symphonia` audio, zero-drift Slide Editor | `render-text`, `core-library`, `media-engine`, `render-wgpu`, `apps/desktop` |
| **M2** | Alpha Key/Fill splitting, dynamic NDI 6 runtime loader, MIT Open Media Transport (OMT) LAN + alpha framing, Blackmagic DeckLink SDI Key/Fill scheduler, Syphon/Spout2 virtual outputs, LAN WebSocket Browser Stage Display | `broadcast-io`, `show-control` (`web_stage`), `apps/desktop` |
| **M3** | Concurrent Timers, cycle-checked Macros, bidirectional MIDI (MultiTracks Playback / Ableton Live presets), OSC (`rosc`), SMPTE LTC/MTC, Bitfocus Companion Satellite, Bibles (OSIS/Zefania/USFX/Beblia + BYOK API.Bible), Planning Center v2 (PKCE + JSON:API + rate governor), CCLI/ChordPro/SongSelect `.usr` & CSV export, clean-room Pro6/Pro7/OpenLyrics/PPTX/LibreOffice/Keynote importers, `loro` CRDT + S3/R2 BLAKE3 asset sync, Two-Tier Plugin Host (`SidecarSupervisor` + fuel-metered `wasmtime` sandbox) | `show-control`, `integrations`, `plugin-sdk`, `plugin-host`, `wit/`, `apps/desktop` |

## Testing & >= 90% Coverage Verification

```powershell
# Rust workspace lints & unit + multi-process resilience tests
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Real LLVM source-based Rust line/region coverage (all 10 library crates)
pwsh -NoProfile -File scripts/check-coverage.ps1

# Svelte 5 + Tauri 2 UI tests (happy + failure paths) with >=90% V8 coverage gate
cd apps/desktop
npm run test:coverage
npm run check
npm run build
```

## Architecture Documentation

- [01 — Architecture Decision Record](docs/architecture/01-architecture-decision-record.md)
- [02 — Workspace, Plugin & License Design](docs/architecture/02-workspace-and-plugin-design.md)
- [03 — Phased Engineering Roadmap](docs/architecture/03-phased-roadmap.md)
- [04 — Resolved Decisions & Risk Register](docs/architecture/04-open-questions-and-risks.md)
