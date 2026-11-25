<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import PreviewCanvas from "./lib/PreviewCanvas.svelte";
  import ArrangementsCuesBar from "./lib/ArrangementsCuesBar.svelte";
  import BroadcastPanel from "./lib/BroadcastPanel.svelte";
  import ContextMenu from "./lib/ContextMenu.svelte";
  import DiagnosticsPanel from "./lib/DiagnosticsPanel.svelte";
  import { openContextMenu, type ContextMenuItem } from "./lib/contextMenuStore.svelte";
  import LayerManager from "./lib/LayerManager.svelte";
  import LibraryPlaylistDrawer from "./lib/LibraryPlaylistDrawer.svelte";
  import LooksTemplatesPanel from "./lib/LooksTemplatesPanel.svelte";
  import MediaBinDrawer from "./lib/MediaBinDrawer.svelte";
  import MenuBar, { type WorkspaceTab } from "./lib/MenuBar.svelte";
  import OverlaysMessagesPanel from "./lib/OverlaysMessagesPanel.svelte";
  import QuickLyricsPanel from "./lib/QuickLyricsPanel.svelte";
  import ScreensPanel from "./lib/ScreensPanel.svelte";
  import ScriptureDrawer from "./lib/ScriptureDrawer.svelte";
  import ShowControlPanel, { type ControlSection } from "./lib/ShowControlPanel.svelte";
  import SlideEditor from "./lib/SlideEditor.svelte";
  import StageDesignerPanel from "./lib/StageDesignerPanel.svelte";
  import StatusBar from "./lib/StatusBar.svelte";
  import UserGuidePanel from "./lib/UserGuidePanel.svelte";
  import { allowDrop, clearAppDrag, readDroppedMedia, startAppDrag } from "./lib/dragStore";
  import type {
    DiagnosticsStatus,
    EngineSnapshot,
    EngineTelemetry,
    LayerKind,
    MaskStyle,
    MediaFitMode,
    OutputConfig,
    SceneState,
    Slide,
    SlideElement,
  } from "./lib/types";

  let snapshot = $state<EngineSnapshot | null>(null);
  let telemetry = $state<EngineTelemetry | null>(null);
  let statusMessage = $state("Connecting to trueshow-engine sidecar...");
  let errorText = $state<string | null>(null);
  let activeTab = $state<WorkspaceTab>("show");
  let activeControlSection = $state<ControlSection>("timers");
  let guideAnchor = $state<string | null>(null);
  let editingTimerSlideId = $state<string | null>(null);
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

  function applySceneState(next: SceneState) {
    errorText = null;
    if (snapshot) snapshot = { ...snapshot, state: next };
  }

  function applyDeckLoaded(next: SceneState) {
    applySceneState(next);
    activeTab = "show";
  }

  function setError(msg: string) {
    errorText = msg;
  }

  function selectTab(tab: WorkspaceTab, section?: ControlSection, anchor?: string) {
    activeTab = tab;
    if (section) activeControlSection = section;
    guideAnchor = anchor ?? null;
  }

  function openSlideContextMenu(event: MouseEvent, slide: Slide) {
    const items: ContextMenuItem[] = [
      { id: "live", label: "Go Live (Trigger Slide)" },
      { id: "edit", label: "Edit in Slide Editor" },
      { id: "timer", label: "Set Auto-Play Timer…" },
      { id: "clear-slide", label: "Clear Slide Layer", shortcut: "F2" },
    ];
    if (slide.media_cue) {
      items.push({ id: "clear-cue", label: `Remove Attached Cue (${slide.media_cue.label})` });
    }
    items.push(
      { id: "blackout", label: scene?.blackout ? "Exit Blackout" : "Blackout", shortcut: "B", separator: true },
      { id: "clear-all", label: "Clear All Layers", shortcut: "F1", danger: true },
    );
    openContextMenu(
      event,
      items,
      (id) => {
        if (id === "live") void triggerSlide(slide.id);
        else if (id === "edit") { void triggerSlide(slide.id); selectTab("editor"); }
        else if (id === "timer") editingTimerSlideId = slide.id;
        else if (id === "clear-slide") void clearLayer("Slide");
        else if (id === "clear-cue") void attachMediaToSlide(slide.id, null);
        else if (id === "blackout") void toggleBlackout();
        else if (id === "clear-all") void clearAll();
      },
      slide.label,
    );
  }

  function openOutputContextMenu(event: MouseEvent, output: OutputConfig) {
    const items: ContextMenuItem[] = [
      { id: "screens", label: "Configure Screen & Monitors" },
      { id: "looks", label: "Manage Looks & Templates" },
    ];
    if (output.kind === "Stage") {
      items.push({ id: "stage", label: "Stage Layout Designer" });
    }
    items.push(
      { id: "freeze", label: scene?.freeze_output ? "Unfreeze Output" : "Freeze Output", shortcut: "F", separator: true },
      { id: "blackout", label: scene?.blackout ? "Exit Blackout" : "Blackout", shortcut: "B" },
      { id: "clear-all", label: "Clear All Layers", shortcut: "F1", danger: true },
    );
    openContextMenu(
      event,
      items,
      (id) => {
        if (id === "screens" || id === "looks" || id === "stage") selectTab(id);
        else if (id === "freeze") void toggleFreeze();
        else if (id === "blackout") void toggleBlackout();
        else if (id === "clear-all") void clearAll();
      },
      `${output.name} (${output.kind})`,
    );
  }

  async function attachEngine() {
    errorText = null;
    statusMessage = "Attaching to trueshow-engine...";
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

  async function toggleFreeze() {
    if (!scene) return;
    errorText = null;
    try {
      const next = await invoke<SceneState>("set_freeze", {
        enabled: !scene.freeze_output,
      });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function undoScene() {
    errorText = null;
    try {
      const next = await invoke<SceneState>("undo_scene");
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function redoScene() {
    errorText = null;
    try {
      const next = await invoke<SceneState>("redo_scene");
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function setMediaFitMode(fitMode: MediaFitMode) {
    if (!scene) return;
    errorText = null;
    const base = scene.media_transport ?? {
      playing: true,
      looping: true,
      muted: true,
      volume: 1,
      fit_mode: "Fill" as MediaFitMode,
      position_secs: 0,
      duration_secs: 0,
    };
    try {
      const next = await invoke<SceneState>("set_media_transport", {
        transport: { ...base, fit_mode: fitMode },
      });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  async function stopAudioPlayback() {
    errorText = null;
    try {
      const next = await invoke<SceneState>("set_audio_playback", { audio: null });
      if (snapshot) snapshot = { ...snapshot, state: next };
    } catch (err) {
      errorText = String(err);
    }
  }

  function stepSlide(delta: number) {
    if (!scene || scene.deck.length === 0) return;
    const curIdx = scene.active_slide
      ? scene.deck.findIndex((s) => s.id === scene.active_slide?.id)
      : -1;
    const nextIdx = Math.max(0, Math.min(scene.deck.length - 1, (curIdx < 0 ? 0 : curIdx) + delta));
    const target = scene.deck[nextIdx];
    if (target) void triggerSlide(target.id);
  }

  function jumpToGroupPrefix(prefix: string) {
    if (!scene) return;
    const hit = scene.deck.find((s) => s.group.toLowerCase().startsWith(prefix.toLowerCase()));
    if (hit) void triggerSlide(hit.id);
  }

  function groupBadgeColor(group: string): string {
    const g = group.toLowerCase();
    if (g.includes("chorus")) return "#2563eb";
    if (g.includes("bridge")) return "#9333ea";
    if (g.includes("verse")) return "#0d9488";
    if (g.includes("scripture")) return "#d97706";
    if (g.includes("pre")) return "#475569";
    return "#334155";
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

    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const tag = target?.tagName ?? "";
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) {
        return;
      }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
        event.preventDefault();
        if (event.shiftKey) void redoScene();
        else void undoScene();
        return;
      }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "y") {
        event.preventDefault();
        void redoScene();
        return;
      }
      if (event.key === "ArrowRight" || event.key === "PageDown" || event.key === " ") {
        event.preventDefault();
        stepSlide(1);
      } else if (event.key === "ArrowLeft" || event.key === "PageUp") {
        event.preventDefault();
        stepSlide(-1);
      } else if (event.key === "F1") {
        event.preventDefault();
        void clearAll();
      } else if (event.key === "F2") {
        event.preventDefault();
        void clearLayer("Slide");
      } else if (event.key.toLowerCase() === "b") {
        void toggleBlackout();
      } else if (event.key.toLowerCase() === "f") {
        void toggleFreeze();
      } else if (event.key.toLowerCase() === "v") {
        jumpToGroupPrefix("verse");
      } else if (event.key.toLowerCase() === "c") {
        jumpToGroupPrefix("chorus");
      } else if (event.key.toLowerCase() === "g") {
        jumpToGroupPrefix("bridge");
      }
    };
    window.addEventListener("keydown", onKeyDown);

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
      window.removeEventListener("keydown", onKeyDown);
    };
  });
</script>

<div class="shell">
  <MenuBar
    {activeTab}
    {activeControlSection}
    {connected}
    frozen={Boolean(scene?.freeze_output)}
    {crashCount}
    {showLibraryDrawer}
    {showLayerDock}
    {showMediaDrawer}
    onSelectTab={selectTab}
    onToggleLibraryDrawer={() => (showLibraryDrawer = !showLibraryDrawer)}
    onToggleLayerDock={() => (showLayerDock = !showLayerDock)}
    onToggleMediaDrawer={() => (showMediaDrawer = !showMediaDrawer)}
    onUndo={() => void undoScene()}
    onRedo={() => void redoScene()}
    onToggleFreeze={() => void toggleFreeze()}
    onReconnect={attachEngine}
    onReloadUi={reloadUiKeepLive}
    onShutdownEngine={shutdownEngine}
    onShutdownAll={shutdownAll}
  />

  {#if errorText}
    <div class="error-banner" role="alert">{errorText}</div>
  {/if}

  <main class="workspace" class:no-library={!showLibraryDrawer}>
    {#if showLibraryDrawer}
      <LibraryPlaylistDrawer onDeckLoaded={applyDeckLoaded} onError={setError} />
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
      {:else if activeTab === "scripture"}
        <ScriptureDrawer onDeckLoaded={applyDeckLoaded} onError={setError} />
      {:else if activeTab === "lyrics" && scene}
        <QuickLyricsPanel {scene} onSceneChange={applySceneState} onError={setError} />
      {:else if activeTab === "overlays" && scene}
        <OverlaysMessagesPanel {scene} onSceneChange={applySceneState} onError={setError} />
      {:else if activeTab === "stage" && scene}
        <StageDesignerPanel {scene} onSceneChange={applySceneState} onError={setError} />
      {:else if activeTab === "looks" && scene}
        <LooksTemplatesPanel {scene} onSceneChange={applySceneState} onError={setError} />
      {:else if activeTab === "broadcast"}
        <BroadcastPanel />
      {:else if activeTab === "ecosystem"}
        <ShowControlPanel
          initialSection={activeControlSection}
          onSectionChange={(s) => (activeControlSection = s)}
          onSceneChange={applySceneState}
        />
      {:else if activeTab === "diagnostics"}
        <DiagnosticsPanel />
      {:else if activeTab === "guide"}
        <UserGuidePanel initialAnchor={guideAnchor} onClose={() => selectTab("show")} />
      {:else if activeTab === "editor" && editableSlide}
        <SlideEditor slide={editableSlide} onUpdateElement={updateSlideElement} onSceneChange={applySceneState} />
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
                Media Fit:
                <select
                  aria-label="Media Fit Mode"
                  value={scene.media_transport?.fit_mode ?? "Fill"}
                  onchange={(e) => setMediaFitMode(e.currentTarget.value as MediaFitMode)}
                >
                  <option value="Fill">Fill</option>
                  <option value="Contain">Contain</option>
                  <option value="Stretch">Stretch</option>
                </select>
              </label>
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
          <ArrangementsCuesBar
            {scene}
            {editingTimerSlideId}
            onSceneChange={applySceneState}
            onTriggerSlide={(id) => void triggerSlide(id)}
            onError={setError}
            onClearTimerTarget={() => (editingTimerSlideId = null)}
          />

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
                draggable="true"
                ondragstart={(e) => startAppDrag(e, { type: "master-group", group: slide.group })}
                ondragend={clearAppDrag}
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
                oncontextmenu={(e) => openSlideContextMenu(e, slide)}
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
                    <span class="group-tag" style={`background:${groupBadgeColor(slide.group)};`}>
                      {slide.group}
                    </span>
                    {#if isLive}
                      <span class="on-air">LIVE</span>
                    {:else if slide.auto_advance_secs}
                      <span class="timer-tag" title="Auto-advance cue timer">
                        {slide.loop_to_first ? "↻" : "⏱"} {slide.auto_advance_secs.toFixed(1)}s
                      </span>
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

          {#if scene.active_audio}
            <div class="audio-bar">
              <span>AUDIO: <strong>{scene.active_audio.title}</strong></span>
              <button class="btn" onclick={() => void stopAudioPlayback()}>Stop Audio</button>
            </div>
          {/if}

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
        <div class="monitor-head-actions">
          <button class="btn" onclick={() => (activeTab = "looks")}>Manage Looks</button>
          <button class="btn" onclick={() => (activeTab = "screens")}>Configure</button>
        </div>
      </div>

      {#if scene}
        <div class="monitors">
          {#each scene.outputs as output (output.id)}
            <div class="output-block" role="group" oncontextmenu={(e) => openOutputContextMenu(e, output)}>
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
    <MediaBinDrawer onMediaTriggered={applySceneState} onError={setError} />
  {/if}

  <StatusBar
    {connected}
    {statusMessage}
    {snapshot}
    {telemetry}
    {crashCount}
    onReconnect={attachEngine}
    onOpenDiagnostics={() => selectTab("diagnostics")}
  />

  <ContextMenu />
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .monitor-head-actions {
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

  .audio-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 10px;
    background: rgba(16, 185, 129, 0.14);
    border: 1px solid rgba(16, 185, 129, 0.4);
    border-radius: 6px;
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

  .timer-tag {
    position: absolute;
    top: 8px;
    right: 8px;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(15, 23, 42, 0.78);
    border: 1px solid rgba(56, 189, 248, 0.5);
    color: #bae6fd;
    font-size: 10px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
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

  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
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
