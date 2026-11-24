# OpenPresenter

Clean-room, open-source, cross-platform (**Windows, macOS, Linux**) worship presentation engine built in **Rust (`wgpu` + `winit`)** with a **Tauri 2 + Svelte 5** operator control surface.

## Architecture Principles

- **Zero Browser-Rendered Live Outputs:** Physical Audience displays, Stage displays, and broadcast feeds (NDI/OMT/SDI/Syphon/Spout) composite directly on the GPU via `wgpu` (`Direct3D 12`, `Metal`, `Vulkan`) inside a decoupled sidecar process (`openpresenter-engine`).
- **Sanctuary Zero-Blackout Resilience:** Killing or reloading the Tauri 2 operator UI never drops physical sanctuary screens to black; the UI re-attaches seamlessly to the running engine.
- **Single Source of Truth:** Operator live previews stream directly from the GPU compositor over lock-free shared memory and Tauri 2 raw binary `Channel<Response>` payloads into a WebGL2 `<canvas>`.
- **Clean-Room & Copyleft:** Core engine crates are licensed under **MPL-2.0** (with planned `plugin-sdk` / WIT interfaces dual-licensed under `MIT OR Apache-2.0`), enforced in CI via `deny.toml`.

## Architecture Docs

- [01 — Architecture Decision Record](docs/architecture/01-architecture-decision-record.md)
- [02 — Workspace, Plugin & License Design](docs/architecture/02-workspace-and-plugin-design.md)
- [03 — Phased Engineering Roadmap](docs/architecture/03-phased-roadmap.md)
- [04 — Resolved Decisions & Risk Register](docs/architecture/04-open-questions-and-risks.md)

## Current Milestone: M0 (Dual-Window GPU Compositor + Sidecar IPC PoC)

### Workspace Layout

- `crates/core-scene` — Pure-Rust canonical scene graph (7 layers, Looks, slides, transitions, deterministic diffs).
- `crates/core-ipc` — Cross-platform `interprocess` framed binary control plane + lock-free triple-buffered `memmap2` shared-memory preview ring.
- `crates/render-wgpu` — `wgpu` 30 + `winit` 30 multi-surface GPU compositor with animated WGSL shader, per-output Looks, and non-blocking 3-slot async staging readback ring.
- `bins/engine` — Standalone `openpresenter-engine` sidecar executable (reconnectable IPC server; stays live across UI restarts).
- `apps/desktop` — Tauri 2 + Svelte 5 operator console streaming live RGBA preview frames over raw binary IPC channels.

### Running M0 Locally

1. Build the engine sidecar & run workspace tests:
   ```bash
   cargo test --workspace
   cargo build --bin openpresenter-engine
   ```
2. Launch the engine standalone (opens two `wgpu` output windows — Audience & Stage — and serves local IPC):
   ```bash
   cargo run --bin openpresenter-engine -- --outputs 2
   ```
3. Launch the Tauri 2 + Svelte 5 operator console (automatically attaches to the running engine or spawns one):
   ```bash
   cd apps/desktop
   npm install
   npm run tauri dev
   ```
