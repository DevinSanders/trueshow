<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import PreviewCanvas from "./lib/PreviewCanvas.svelte";
  import BroadcastPanel from "./lib/BroadcastPanel.svelte";
  import DiagnosticsPanel from "./lib/DiagnosticsPanel.svelte";
  import LayerManager from "./lib/LayerManager.svelte";
  import LibraryPlaylistDrawer from "./lib/LibraryPlaylistDrawer.svelte";
  import MediaBinDrawer from "./lib/MediaBinDrawer.svelte";
  import ScreensPanel from "./lib/ScreensPanel.svelte";
  import ShowControlPanel from "./lib/ShowControlPanel.svelte";
  import SlideEditor from "./lib/SlideEditor.svelte";
  import { allowDrop, readDroppedMedia } from "./lib/dragStore";
  import type {
    DiagnosticsStatus,
    EngineSnapshot,
    EngineTelemetry,
    LayerKind,
    MaskStyle,
    SceneState,
    SlideElement,
  } from "./lib/types";

  let snapshot = $state<EngineSnapshot | null>(null);
  let telemetry = $state<EngineTelemetry | null>(null);
  let statusMessage = $state("Connecting to openpresenter-engine sidecar...");
  let errorText = $state<string | null>(null);
  let activeTab = $state<"show" | "screens" | "editor" | "broadcast" | "ecosystem" | "diagnostics">("show");
  let crashCount = $state(0);
  let showLibraryDrawer = $state(true);
  let showLayerDock = $state(true);
  let showMediaDrawer = $state(true);

  let transitionKind = $state<"Cut" | "Dissolve" | "PushLeft" | "WipeRight">("Dissolve");
  let dissolveMs = $state(450);
  let stageMessageInput = $state("");
  let dragOverSlideId = $state<string | null>(null);

  const connected = $derived(snapshot !== null);
  const scene = $derived(snapshot?.state ?? null);
  const looksList = $derived(scene ? Object.values(scene.looks) : []);
  const editableSlide = $derived(scene?.active_slide ?? scene?.deck[0] ?? null);

  async function attachEngine() {
    errorText = null;
    statusMessage = "Attaching to openpresenter-engine...";
    try {
      const snap = await invoke<EngineSnapshot>("connect_engine");
      snapshot = snap;
      telemetry = snap.telemetry;
      statusMessage = `Attached to engine PID ${snap.engine_pid}`;
    } catch (err) {
      errorText = String(err);
      statusMessage = "Engine unreachable";
    }
  }

  async function triggerSlide(slideId: string) {
    errorText = null;
    try {
      const next = await invoke<SceneState>("trigger_slide", {
        slideId,
        transitionKind,
        durationMs: transitionKind === "Cut" ? 0 : dissolveMs,
      });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function updateSlideElement(slideId: string, element: SlideElement) {
    const next = await invoke<SceneState>("update_slide_element", {
      slideId,
      element,
    });
    if (snapshot) snapshot = { ...snapshot, state: next };
  }

  async function clearLayer(layer: LayerKind) {
    errorText = null;
    try {
      const next = await invoke<SceneState>("clear_layer", { layer });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function clearAll() {
    errorText = null;
    try {
      const next = await invoke<SceneState>("clear_all");
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function toggleBlackout() {
    if (!scene) return;
    errorText = null;
    try {
      const next = await invoke<SceneState>("set_blackout", {
        enabled: !scene.blackout,
      });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function toggleOverlay() {
    if (!scene) return;
    errorText = null;
    try {
      const next = await invoke<SceneState>("toggle_overlay", {
        active: !scene.overlay_layer_active,
      });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function setMask(mask: MaskStyle) {
    errorText = null;
    try {
      const next = await invoke<SceneState>("set_mask", { mask });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function sendStageMessage() {
    errorText = null;
    const msg = stageMessageInput.trim();
    try {
      const next = await invoke<SceneState>("set_stage_message", {
        message: msg.length > 0 ? msg : null,
      });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function setOutputLook(outputId: string, lookId: string) {
    errorText = null;
    try {
      const next = await invoke<SceneState>("set_output_look", {
        outputId,
        lookId,
      });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function toggleVideoInput() {
    if (!scene) return;
    errorText = null;
    try {
      const next = await invoke<SceneState>("toggle_video_input", {
        active: !scene.video_input_active,
      });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function triggerMediaOnLayer(
    blake3Hash: string,
    role: "background" | "foreground" | "announcement",
  ) {
    errorText = null;
    try {
      const next = await invoke<SceneState>("trigger_media_asset", { blake3Hash, role });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function attachMediaToSlide(slideId: string, blake3Hash: string | null) {
    errorText = null;
    try {
      const next = await invoke<SceneState>("attach_media_to_slide", {
        slideId,
        blake3Hash,
        role: "auto",
      });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  function handleDropMediaOnSlide(event: DragEvent, slideId: string) {
    dragOverSlideId = null;
    event.preventDefault();
    const media = readDroppedMedia(event);
    if (media?.blake3_hash) {
      void attachMediaToSlide(slideId, media.blake3_hash);
    }
  }

  function reloadUiKeepLive() {
    window.location.reload();
  }

  async function shutdownEngine() {
    errorText = null;
    try {
      await invoke("shutdown_engine");
      snapshot = null;
      telemetry = null;
      statusMessage = "Engine shut down cleanly";
    } catch (err) {
      errorText = String(err);
    }
  }

  async function shutdownAll() {
    errorText = null;
    try {
      await invoke("shutdown_all");
      snapshot = null;
      telemetry = null;
      statusMessage = "Shutting down all windows and engines...";
    } catch (err) {
      errorText = String(err);
    }
  }

  function cssRgb(c: { r: number; g: number; b: number }): string {
    return `rgb(${Math.round(c.r * 255)}, ${Math.round(c.g * 255)}, ${Math.round(c.b * 255)})`;
  }

  onMount(() => {
    void attachEngine();
    void invoke<DiagnosticsStatus>("get_diagnostics_status")
      .then((d) => {
        if (d) crashCount = d.crash_count;
      })
      .catch(() => {});

    const onWindowError = (event: ErrorEvent) => {
      void invoke("report_frontend_crash", {
        kind: "ui-window-error",
        message: event.message || "Unhandled window error",
        stack: event.error instanceof Error ? (event.error.stack ?? null) : null,
      }).catch(() => {});
      crashCount += 1;
    };
    const onUnhandledRejection = (event: PromiseRejectionEvent) => {
      const reason = event.reason;
      void invoke("report_frontend_crash", {
        kind: "ui-unhandled-rejection",
        message: reason instanceof Error ? reason.message : String(reason),
        stack: reason instanceof Error ? (reason.stack ?? null) : null,
      }).catch(() => {});
      crashCount += 1;
    };
    window.addEventListener("error", onWindowError);
    window.addEventListener("unhandledrejection", onUnhandledRejection);

    let consecutiveErrors = 0;
    let telemetryInFlight = false;
    const timer = setInterval(async () => {
      if (!snapshot || telemetryInFlight) return;
      telemetryInFlight = true;
      try {
        telemetry = await invoke<EngineTelemetry>("get_telemetry");
        consecutiveErrors = 0;
      } catch (err) {
        consecutiveErrors += 1;
        if (consecutiveErrors >= 3) {
          errorText = `Engine connection lost (${String(err)}). Click Reconnect to re-attach.`;
          statusMessage = "Engine disconnected";
        }
      } finally {
        telemetryInFlight = false;
      }
    }, 1000);
    return () => {
      clearInterval(timer);
      window.removeEventListener("error", onWindowError);
      window.removeEventListener("unhandledrejection", onUnhandledRejection);
    };
  });
</script>

<div class="shell">
  <header class="topbar">
    <div class="brand">
      <span class="dot" class:live={connected}></span>
      <strong>OpenPresenter</strong>
      <span class="pill">M3 Complete</span>
      <nav class="tabs" aria-label="Workspace views">
        <button class="tab" class:active={activeTab === "show"} onclick={() => (activeTab = "show")}>
          Show
        </button>
        <button class="tab" class:active={activeTab === "screens"} onclick={() => (activeTab = "screens")}>
          Screens
        </button>
        <button class="tab" class:active={activeTab === "editor"} onclick={() => (activeTab = "editor")}>
          Slide Editor
        </button>
        <button class="tab" class:active={activeTab === "broadcast"} onclick={() => (activeTab = "broadcast")}>
          Broadcast I/O
        </button>
        <button class="tab" class:active={activeTab === "ecosystem"} onclick={() => (activeTab = "ecosystem")}>
          Control & Sync
        </button>
        <button class="tab" class:active={activeTab === "diagnostics"} onclick={() => (activeTab = "diagnostics")}>
          Diagnostics{#if crashCount > 0} ({crashCount}){/if}
        </button>
      </nav>
    </div>

    <div class="telemetry">
      {#if snapshot}
        <span>GPU: <strong>{snapshot.gpu_adapter_name}</strong> ({snapshot.gpu_backend})</span>
        <span>·</span>
        <span>Engine: <strong>{telemetry ? telemetry.fps.toFixed(1) : "—"} fps</strong></span>
        <span>·</span>
        <span>p99: <strong>{telemetry ? telemetry.p99_frame_ms.toFixed(2) : "—"} ms</strong></span>
      {:else}
        <span>{statusMessage}</span>
      {/if}
    </div>

    <div class="top-actions">
      <button
        class="btn"
        class:toggled={showLibraryDrawer}
        onclick={() => (showLibraryDrawer = !showLibraryDrawer)}
      >
        Libraries Drawer
      </button>
      <button
        class="btn"
        class:toggled={showLayerDock}
        onclick={() => (showLayerDock = !showLayerDock)}
      >
        Layers
      </button>
      <button
        class="btn"
        class:toggled={showMediaDrawer}
        onclick={() => (showMediaDrawer = !showMediaDrawer)}
      >
        Media Bin
      </button>
      <button class="btn" onclick={attachEngine}>Reconnect</button>
      <button class="btn" onclick={reloadUiKeepLive}>Reload UI (Keep Live)</button>
      <button class="btn danger" onclick={shutdownEngine} disabled={!connected}>
        Stop Engine
      </button>
      <button
        class="btn danger-solid"
        onclick={shutdownAll}
        title="Stop all rendering engines and close every window"
      >
        Shutdown All
      </button>
    </div>
  </header>

  {#if errorText}
    <div class="error-banner" role="alert">{errorText}</div>
  {/if}

  <main class="workspace" class:no-library={!showLibraryDrawer}>
    {#if showLibraryDrawer}
      <LibraryPlaylistDrawer
        onDeckLoaded={(next) => {
          errorText = null;
          if (snapshot) snapshot = { ...snapshot, state: next };
          activeTab = "show";
        }}
        onError={(msg) => {
          errorText = msg;
        }}
      />
    {/if}

    <section class="deck-pane">
      {#if activeTab === "screens" && snapshot}
        <ScreensPanel
          {snapshot}
          onSnapshotChange={(next) => {
            snapshot = next;
            telemetry = next.telemetry;
          }}
        />
      {:else if activeTab === "broadcast"}
        <BroadcastPanel />
      {:else if activeTab === "ecosystem"}
        <ShowControlPanel />
      {:else if activeTab === "diagnostics"}
        <DiagnosticsPanel />
      {:else if activeTab === "editor" && editableSlide}
        <SlideEditor slide={editableSlide} onUpdateElement={updateSlideElement} />
      {:else}
        <div class="pane-header">
          <h2>Cue Deck (7-Layer GPU Compositor)</h2>
          <div class="transition-controls">
            <label>
              Transition:
              <select aria-label="Transition Kind" bind:value={transitionKind}>
                <option value="Dissolve">Dissolve</option>
                <option value="PushLeft">Push Left</option>
                <option value="WipeRight">Wipe Right</option>
                <option value="Cut">Cut</option>
              </select>
            </label>
            <label class:dim={transitionKind === "Cut"}>
              {dissolveMs} ms
              <input
                aria-label="Transition Duration"
                type="range"
                min="100"
                max="1500"
                step="50"
                bind:value={dissolveMs}
                disabled={transitionKind === "Cut"}
              />
            </label>
            {#if scene}
              <label>
                Mask:
                <select
                  aria-label="Mask Style"
                  value={scene.mask_style}
                  onchange={(e) => setMask(e.currentTarget.value as MaskStyle)}
                >
                  <option value="None">None</option>
                  <option value="SoftVignette">Soft Vignette</option>
                  <option value="RoundedFrame">Rounded Frame</option>
                  <option value="LowerThirdBand">Lower Third Band</option>
                </select>
              </label>
            {/if}
          </div>
        </div>

        {#if scene}
          <div class="slide-grid">
            {#each scene.deck as slide (slide.id)}
              {@const isLive = scene.active_slide?.id === slide.id && !scene.blackout}
              {@const swatchPrimary =
                slide.media_cue?.role === "Background"
                  ? slide.media_cue.primary_color
                  : slide.primary_color}
              {@const swatchSecondary =
                slide.media_cue?.role === "Background"
                  ? slide.media_cue.secondary_color
                  : slide.secondary_color}
              <div
                class="slide-card-wrap"
                class:drag-over={dragOverSlideId === slide.id}
                role="group"
                ondragenter={(e) => {
                  if (allowDrop(e, ["media"], "copy")) dragOverSlideId = slide.id;
                }}
                ondragover={(e) => {
                  if (allowDrop(e, ["media"], "copy")) dragOverSlideId = slide.id;
                }}
                ondragleave={() => {
                  if (dragOverSlideId === slide.id) dragOverSlideId = null;
                }}
                ondrop={(e) => handleDropMediaOnSlide(e, slide.id)}
              >
                <button
                  class="slide-card"
                  class:active={isLive}
                  onclick={() => triggerSlide(slide.id)}
                >
                  <div
                    class="swatch"
                    style={`background: linear-gradient(135deg, ${cssRgb(swatchPrimary)}, ${cssRgb(swatchSecondary)});`}
                  >
                    <span class="group-tag">{slide.group}</span>
                    {#if isLive}
                      <span class="on-air">LIVE</span>
                    {/if}
                    <div class="swatch-headline">{slide.headline}</div>
                    <div class="swatch-accent" style={`background:${cssRgb(slide.accent_color)}`}></div>
                  </div>
                  <div class="card-footer">
                    <div class="card-label">{slide.label}</div>
                    <div class="card-body">{slide.body}</div>
                  </div>
                </button>
                {#if slide.media_cue}
                  <div class="cue-pill">
                    <span>{slide.media_cue.role.toUpperCase()}: {slide.media_cue.label}</span>
                    <button
                      class="cue-clear"
                      aria-label={`Remove media cue from ${slide.label}`}
                      onclick={() => void attachMediaToSlide(slide.id, null)}
                    >
                      ×
                    </button>
                  </div>
                {/if}
              </div>
            {/each}
          </div>

          <div class="stage-msg-bar">
            <input
              aria-label="Stage Message"
              placeholder="Send urgent message to Stage Display (e.g., Nursery #14)..."
              bind:value={stageMessageInput}
            />
            <button class="btn" onclick={sendStageMessage}>Send Stage Msg</button>
          </div>
        {:else}
          <div class="empty-state">
            <p>{statusMessage}</p>
            <button class="btn primary" onclick={attachEngine}>Launch / Attach Engine</button>
          </div>
        {/if}
      {/if}
    </section>

    <aside class="monitor-pane">
      <div class="pane-header">
        <h2>Native GPU Monitors</h2>
        <button class="btn" onclick={() => (activeTab = "screens")}>Configure</button>
      </div>

      {#if scene}
        <div class="monitors">
          {#each scene.outputs as output (output.id)}
            <div class="output-block">
              <PreviewCanvas
                outputId={output.id}
                label={output.name}
                kind={output.kind}
                {connected}
                enginePid={snapshot?.engine_pid ?? 0}
              />
              <div class="look-row">
                <span>Look:</span>
                <select
                  aria-label={`Look for ${output.name}`}
                  value={output.look_id}
                  onchange={(e) => setOutputLook(output.id, e.currentTarget.value)}
                >
                  {#each looksList as look (look.id)}
                    <option value={look.id}>{look.name}</option>
                  {/each}
                </select>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </aside>
  </main>

  {#if showLayerDock && scene}
    <LayerManager
      {scene}
      onClearLayer={(layer) => void clearLayer(layer)}
      onClearAll={() => void clearAll()}
      onToggleBlackout={() => void toggleBlackout()}
      onToggleOverlay={() => void toggleOverlay()}
      onToggleVideoInput={() => void toggleVideoInput()}
      onDropMediaToLayer={(hash, role) => void triggerMediaOnLayer(hash, role)}
    />
  {/if}

  {#if showMediaDrawer}
    <MediaBinDrawer
      onMediaTriggered={(next) => {
        errorText = null;
        if (snapshot) snapshot = { ...snapshot, state: next };
      }}
      onError={(msg) => {
        errorText = msg;
      }}
    />
  {/if}
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 14px;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    font-size: 13px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .tabs {
    display: flex;
    gap: 4px;
    margin-left: 8px;
  }

  .tab {
    padding: 4px 10px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-1);
    font-size: 12px;
    font-weight: 600;
  }

  .tab.active {
    background: var(--bg-2);
    border-color: var(--border);
    color: var(--text-0);
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--warn);
  }

  .dot.live {
    background: var(--ok);
    box-shadow: 0 0 8px var(--ok);
  }

  .pill {
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-size: 11px;
  }

  .telemetry {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-1);
    font-variant-numeric: tabular-nums;
  }

  .telemetry strong {
    color: var(--text-0);
  }

  .top-actions {
    display: flex;
    gap: 8px;
  }

  .error-banner {
    background: rgba(239, 68, 68, 0.16);
    border-bottom: 1px solid rgba(239, 68, 68, 0.45);
    color: #fecaca;
    padding: 8px 16px;
    font-size: 12px;
  }

  .workspace {
    flex: 1;
    display: grid;
    grid-template-columns: 270px 1fr 360px;
    min-height: 0;
  }

  .workspace.no-library {
    grid-template-columns: 1fr 360px;
  }

  .stage-msg-bar input {
    width: 100%;
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font-size: 12px;
  }

  .deck-pane {
    display: flex;
    flex-direction: column;
    padding: 14px 16px;
    gap: 12px;
    overflow: hidden;
  }

  .monitor-pane {
    border-left: 1px solid var(--border);
    background: #0d1119;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
  }

  .pane-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .pane-header h2 {
    margin: 0;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-1);
  }

  .transition-controls {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    color: var(--text-1);
  }

  .transition-controls select {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 6px;
    font-size: 12px;
  }

  .transition-controls label {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .transition-controls label.dim {
    opacity: 0.45;
  }

  .slide-grid {
    flex: 1;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 10px;
    align-content: start;
    overflow-y: auto;
    padding-right: 4px;
  }

  .slide-card-wrap {
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-radius: 9px;
  }

  .slide-card-wrap.drag-over {
    outline: 2px dashed #38bdf8;
    background: rgba(56, 189, 248, 0.12);
  }

  .cue-pill {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
    padding: 3px 7px;
    background: rgba(37, 99, 235, 0.22);
    border: 1px solid rgba(59, 130, 246, 0.45);
    border-radius: 5px;
    font-size: 10px;
    color: #bfdbfe;
  }

  .cue-clear {
    background: transparent;
    border: none;
    color: #fca5a5;
    cursor: pointer;
    font-size: 12px;
    padding: 0 3px;
  }

  .slide-card {
    background: var(--bg-1);
    border: 2px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    padding: 0;
    text-align: left;
    color: var(--text-0);
  }

  .slide-card.active {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-glow);
  }

  .swatch {
    position: relative;
    height: 102px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
  }

  .group-tag {
    align-self: flex-start;
    padding: 2px 7px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.52);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .on-air {
    position: absolute;
    top: 8px;
    right: 8px;
    padding: 2px 7px;
    border-radius: 4px;
    background: var(--danger);
    color: #fff;
    font-size: 10px;
    font-weight: 800;
  }

  .swatch-headline {
    font-size: 13px;
    font-weight: 700;
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.8);
  }

  .swatch-accent {
    height: 3px;
    width: 48px;
    border-radius: 2px;
  }

  .card-footer {
    padding: 8px 10px;
  }

  .card-label {
    font-size: 12px;
    font-weight: 600;
  }

  .card-body {
    margin-top: 2px;
    font-size: 11px;
    color: var(--text-1);
  }

  .stage-msg-bar {
    display: flex;
    gap: 8px;
  }

  .btn {
    padding: 6px 11px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }

  .btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
  }

  .btn.toggled {
    border-color: var(--ok);
    color: #bbf7d0;
  }

  .btn.danger {
    border-color: rgba(239, 68, 68, 0.5);
    color: #fca5a5;
  }

  .btn.danger-solid {
    background: #b91c1c;
    border-color: #ef4444;
    color: #fff;
    font-weight: 600;
  }

  .monitors {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .output-block {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .look-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-1);
  }

  .look-row select {
    flex: 1;
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
    font-size: 12px;
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--text-1);
  }
</style>
