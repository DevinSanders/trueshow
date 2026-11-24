# Open Questions, Challenged Assumptions & Architectural Risks

> **Status:** Resolved — Ready for M0 Initialization  
> **Date:** November 2026  
> **Resolution Summary:**
> 1. **Core License:** Copyleft confirmed (**MPL-2.0** for core workspace crates + **MIT OR Apache-2.0** for `plugin-sdk`/`wit/`, keeping NDI/DeckLink/LGPL legally clean; DeckLink & ASIO isolated behind opt-in features/sidecars).
> 2. **Typography Stack:** Accepted **`skia-safe`** CPU-cached rasterizer behind `render-text::Vector2DRasterizer` trait.
> 3. **Operator UI Framework:** Accepted **Svelte 5 + TypeScript** inside Tauri 2.
> 4. **Windows Media Packaging:** Accepted reproducibly built, dynamically linked **LGPL-2.1+ FFmpeg** via `rsmpeg`.
> 5. **MultiTracks & Windows GPU Fallback:** **MultiTracks MCP dropped** (no workflows gated behind AI agents; MIDI cues + Planning Center only). Two-tier Windows D3D12VA/Spout2 zero-copy + async staging fallback and OS/GPU baseline accepted.
> **Companion Docs:** [01-architecture-decision-record.md](./01-architecture-decision-record.md) | [02-workspace-and-plugin-design.md](./02-workspace-and-plugin-design.md) | [03-phased-roadmap.md](./03-phased-roadmap.md)

---

## Top 5 Decisions Needed Before Initializing the Repository & Starting M0

Before scaffolding Cargo crates or writing application code, we should explicitly resolve these five architectural forks and validate several key assumptions from the project kickoff.

---

### 1. Core License Finalization (`MPL-2.0` vs. `GPL-3.0 + Linking Exception`) & Proprietary SDK Build Boundaries

**Assumption Interrogated:** The kickoff leaves the core license open between `MPL-2.0` and `GPL-3.0 with a linking exception`, while also targeting Blackmagic DeckLink SDI and optional low-latency Windows audio.

**Why This Matters Now:**
- **GPL-3.0 + custom linking exception** creates recurring legal and contributor friction: every contributor must grant the exact exception text, and dynamic loading of proprietary NDI runtimes, Blackmagic DeckLink headers, or commercial/MIT plugins remains legally scrutinized by downstream packagers.
- **MPL-2.0** protects every core OpenPresenter source file from closed-source proprietary forks while natively allowing dynamic linking to LGPL-2.1+ FFmpeg, runtime loading of proprietary NDI/DeckLink drivers, and permissive (`MIT OR Apache-2.0`) third-party plugins.
- **Blackmagic DeckLink CI Question:** Even under MPL-2.0, Blackmagic's DeckLink SDK headers carry a proprietary EULA (drivers cannot be redistributed, and developers must download the SDK from Blackmagic). Public CI cannot check DeckLink SDK headers into a clean-room repo.

**Decision Needed:**
- **Option A (Recommended):** Commit to **`MPL-2.0`** for core crates + **`MIT OR Apache-2.0`** for `crates/plugin-sdk` & `wit/`, and isolate Blackmagic DeckLink (and Steinberg ASIO, if ever added) behind opt-in Cargo features / separately packaged official Tier 1 Sidecar plugins.
- **Option B:** Use `GPL-3.0-only` with a lawyer-drafted linking exception covering NDI, DeckLink, and plugins.

---

### 2. Typography Engine Trade-Off: `skia-safe` (Day-1 Broadcast Polish) vs. Pure-Rust `vello` + `parley`

**Assumption Interrogated:** Choosing `wgpu` for compositing makes a pure-Rust `wgpu` text stack (`vello` + `parley`/`cosmic-text`) tempting on paper, whereas broadcast worship projection heavily relies on crisp outside text outlines, multi-pass Gaussian drop shadows, glows, complex RTL/CJK shaping, and rock-solid editor caret hit-testing.

**The Trade-Off:**
- **`skia-safe` (CPU-rasterized, GPU-cached behind `Vector2DRasterizer` trait):** Delivers 100% of ProPresenter-grade typography on Day 1 with zero GPU context conflicts against `wgpu` 30.x, and prebuilt binaries via `skia-bindings` keep contributor builds fast. **Cost:** Adds ~15–25 MB to binary size and introduces a C++ binary dependency (BSD-3 + MIT).
- **Pure-Rust `vello` + `parley` + custom WGSL blur/stroke shaders:** Zero C++ dependency and smaller binaries. **Cost:** Adds an estimated **4–6 weeks** to M1 to build broadcast-grade outside text strokes, Gaussian shadow/glow passes, and fight `vello` vs `wgpu` major-version lockstep lag.

**Decision Needed:**
- **Option A (Recommended):** Ship M1 with **`skia-safe` CPU-cached rasterization** behind an isolated `render-text` trait, preserving a clean future swap path to `vello` + `parley`.
- **Option B:** Enforce a strict 100% pure-Rust text stack (`vello` + `parley` / `cosmic-text`) from Day 1 and accept the extra M1 schedule cost.

---

### 3. Operator UI Framework: `Svelte 5` (Technical Optimum) vs. `React 19` (Contributor Pool Optimum)

**Assumption Interrogated:** Both Svelte 5 and React 19 work cleanly with Tauri 2 (`v2.12+`) and TypeScript.

**The Trade-Off:**
- **Svelte 5 (Runes):** Technically superior for a live show-control dashboard — ~2–5 KB base runtime, surgical DOM updates for 10–60 Hz timers/VU meters/telemetry without VDOM diffing or GC spikes.
- **React 19 (Compiler):** Slightly heavier (~42–45 KB base runtime), but taps into a dramatically larger pool of open-source contributors, church tech volunteers, and mature desktop component libraries.

**Decision Needed:**
- **Option A (Recommended):** Standardize on **Svelte 5 + TypeScript** for maximum live-machine efficiency and clean Tauri 2 ergonomics.
- **Option B:** Standardize on **React 19 + TypeScript** if maximizing third-party OSS contributor familiarity outweighs Svelte's runtime efficiency.

---

### 4. FFmpeg LGPL Dynamic Bundling vs. Strict OS-Native Media APIs on Windows

**Assumption Interrogated:** The kickoff calls for avoiding patent-encumbered software encoders *and* supporting **ProRes 4444 + Alpha** and **WebM VP9 + Alpha** alongside hardware H.264/H.265 across Windows and macOS.

**The Technical Catch:**
- On **macOS**, Apple's VideoToolbox natively hardware-decodes H.264, HEVC, and ProRes 4444 (including alpha on Apple Silicon).
- On **Windows**, Media Foundation does **not** natively decode ProRes 4444 with alpha or WebM VP9+Alpha side-streams, and hardware decoders (D3D12VA/NVDEC) only cover H.264/HEVC/VP9-opaque/AV1.
- Therefore, achieving full cross-platform motion-background & alpha-video parity requires **dynamically linked LGPL-2.1+ FFmpeg shared libraries** (compiled strictly without `--enable-gpl`, `--enable-nonfree`, `x264`, or `x265`) via `rsmpeg`, adding ~25–40 MB to installers and requiring a reproducible LGPL FFmpeg build pipeline in CI.

**Decision Needed:**
- **Option A (Recommended):** Ship **reproducibly built, dynamically linked LGPL-2.1+ FFmpeg libraries** (hardware decode for H.264/HEVC via VideoToolbox/D3D12VA/VA-API + LGPL software decode for ProRes 4444 & VP9+alpha) with full LGPL compliance notices.
- **Option B:** Avoid bundling FFmpeg completely (rely on system-installed FFmpeg or OS-native APIs only), accepting degraded out-of-the-box alpha-video support on Windows unless users manually install FFmpeg.

---

### 5. MultiTracks MCP Access Gate & Windows Zero-Copy `wgpu` D3D12 Interop Risk

**Two Linked High-Impact Technical Unknowns Uncovered During 2026 Research:**

1. **MultiTracks MCP (`https://mcp.multitracks.com/mcp`) Auth Reality:**
   - Research confirms MultiTracks' MCP server requires an active **MultiTracks One** or **Live Bundle** subscription and currently documents **client-specific OAuth Client IDs** (`mcp-claude-desktop`, `mcp-chatgpt` + client secret). It is **unverified** whether MultiTracks currently permits open third-party OAuth Dynamic Client Registration (DCR) for independent open-source apps.
   - **Recommended Policy:** Make **inbound MIDI cues from Playback / Ableton Live** + **Planning Center setlist sync** the guaranteed primary open path in M3, while building the MultiTracks MCP connector as an experimental/opt-in integration pending formal MultiTracks OAuth registration or DCR verification.

2. **Windows Zero-Copy `wgpu` D3D12 Interop (`D3D12VA` Video + `Spout2`):**
   - Unlike macOS (`IOSurface` -> Metal -> `wgpu-hal` + `syphon-wgpu`, which is clean and battle-tested), Windows `wgpu` 30.x zero-copy external texture import requires unsafe `wgpu-hal::dx12` interop (`ID3D12Resource` fence sync for D3D12VA and `D3D11On12` / DXGI shared NT handles for Spout2). Certain older Windows GPU drivers (integrated Intel/AMD laptops common in smaller churches) exhibit driver-level D3D12VA quirks.
   - **Recommended Policy:** Commit to a **Two-Tier GPU Path on Windows** — attempt true zero-copy D3D12VA / D3D11On12 first, with automatic, seamless runtime fallback to a lock-free async staging-buffer upload path (keeping YUV/YUVA conversion on GPU shaders) so older church laptops degrade gracefully instead of crashing or showing black screens.

**Decision Needed:**
- Confirm whether you approve **both** fallback policies (MIDI-first for MultiTracks pending MCP OAuth verification; automatic staging-ring fallback for Windows D3D12VA/Spout2 on legacy GPU drivers), and confirm the **minimum supported GPU / OS floor** we should target (Recommended: **Windows 10 22H2 / Windows 11** with D3D12 Feature Level 12_0+, **macOS 14 Sonoma+** Apple Silicon & Intel Metal 3, **Linux** Vulkan 1.2+).

---

## Secondary Architectural Risks & Mitigations Register

| Risk | Likelihood | Impact | Concrete Mitigation |
| :--- | :---: | :---: | :--- |
| **macOS Hardened Runtime blocks dynamic NDI / DeckLink / FFmpeg loading after notarization** | High | High | Sign with `com.apple.security.cs.disable-library-validation` entitlement (required on macOS for loading user-installed third-party `.dylib`/frameworks like NDI Runtime & DeckLink Desktop Video); test notarized `.app` bundles starting in M2, not at v1.0. |
| **Display index / EDID reshuffling on projector sleep/wake** | High | Critical | Persist physical monitor bindings by hardware EDID serial + connector topology (not OS monitor index `0/1/2`); auto-rebind output windows on `winit` display-change events without dropping audio or other outputs. |
| **Multi-GPU laptop adapter mismatch (Windows Optimus / hybrid graphics)** | Medium | High | Explicitly select the high-performance discrete GPU adapter (or the adapter driving the external HDMI/DP outputs) when initializing `wgpu` and D3D12VA so `Spout2` and `D3D12VA` never split across iGPU vs. dGPU heaps. |
| **Open Media Transport (`libomt` / `libomtnet`) packaging friction** | Medium | Medium | Official `libomt` wraps `libomtnet` (.NET NativeAOT), which can add platform-specific native-AOT packaging quirks on macOS notarization. Spike `libomt` NativeAOT binary vs. pure-Rust `openmediatransport`/`vmx-rs` wire interoperability against OBS/vMix in early M2 before committing. |
| **API.Bible contractual cache violation (>30 days offline)** | Medium | Medium | Enforce hard TTL metadata in SQLite per cached API.Bible passage (`fetched_at`): warn UI at day 14, auto-refresh when online, and purge/block display past day 30 with clear guidance to reconnect or switch to an offline public-domain Bible translation. |
| **ProPresenter 7 `.pro` schema drift across minor Renewed Vision updates** | Medium | Low | Protobuf wire format is forward-compatible for unknown fields; design the clean-room `prost` importer to decode only core resilient wire tags (slides, text runs, RTF/plain text, arrangements, CCLI metadata) and skip unknown proprietary fields cleanly without failing the import. |
