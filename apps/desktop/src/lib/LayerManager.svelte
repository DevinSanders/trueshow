<script lang="ts">
  import type { LayerKind, SceneState } from "./types";

  interface Props {
    scene: SceneState;
    onClearLayer: (layer: LayerKind) => void;
    onClearAll: () => void;
    onToggleBlackout: () => void;
    onToggleOverlay: () => void;
    onToggleVideoInput: () => void;
    onDropMediaToLayer?: (blake3Hash: string, role: "background" | "foreground" | "announcement") => void;
  }

  let {
    scene,
    onClearLayer,
    onClearAll,
    onToggleBlackout,
    onToggleOverlay,
    onToggleVideoInput,
    onDropMediaToLayer,
  }: Props = $props();

  let dragOverLayer = $state<LayerKind | null>(null);

  interface LayerRow {
    index: number;
    kind: LayerKind;
    name: string;
    active: boolean;
    summary: string;
    dropRole?: "background" | "foreground" | "announcement";
  }

  const layers = $derived<LayerRow[]>([
    {
      index: 6,
      kind: "MessagesAndTimers",
      name: "Messages & Timers",
      active: Boolean(scene.stage_message || scene.timer_readout),
      summary:
        [scene.timer_readout, scene.stage_message].filter(Boolean).join(" · ") || "Empty",
    },
    {
      index: 5,
      kind: "Masks",
      name: "Masks",
      active: scene.mask_style !== "None",
      summary: scene.mask_style !== "None" ? scene.mask_style : "None",
    },
    {
      index: 4,
      kind: "PropsAndOverlays",
      name: "Props & Overlays",
      active: scene.overlay_layer_active,
      summary: scene.overlay_layer_active ? "Corner Overlay Bug Active" : "Empty",
    },
    {
      index: 3,
      kind: "Announcements",
      name: "Announcements",
      active:
        scene.announcements_layer_active ||
        scene.active_foreground_media?.target_layer === "Announcements",
      summary:
        scene.active_foreground_media?.target_layer === "Announcements"
          ? `FG: ${scene.active_foreground_media.label}`
          : scene.announcement_text || "Empty (drop media here)",
      dropRole: "announcement",
    },
    {
      index: 2,
      kind: "Slide",
      name: "Slide / Foreground",
      active: scene.slide_layer_active,
      summary:
        scene.active_foreground_media?.target_layer === "Slide"
          ? `FG Image/Media: ${scene.active_foreground_media.label}`
          : scene.slide_layer_active && scene.active_slide
            ? scene.active_slide.label
            : "Empty (drop foreground media)",
      dropRole: "foreground",
    },
    {
      index: 1,
      kind: "MediaBackground",
      name: "Media / Background",
      active: scene.media_layer_active && Boolean(scene.active_background),
      summary:
        scene.media_layer_active && scene.active_background
          ? scene.active_background.label
          : "Empty (drop background here)",
      dropRole: "background",
    },
    {
      index: 0,
      kind: "VideoInput",
      name: "Video Input",
      active: scene.video_input_active,
      summary: scene.video_input_active ? "Live NDI / Camera Input Active" : "Off",
    },
  ]);

  function handleDragOver(event: DragEvent, row: LayerRow) {
    if (!row.dropRole) return;
    event.preventDefault();
    dragOverLayer = row.kind;
  }

  function handleDrop(event: DragEvent, row: LayerRow) {
    dragOverLayer = null;
    if (!row.dropRole || !onDropMediaToLayer) return;
    event.preventDefault();
    const raw = event.dataTransfer?.getData("application/x-openpresenter-media");
    if (!raw) return;
    try {
      const parsed = JSON.parse(raw) as { blake3_hash?: string };
      if (parsed.blake3_hash) {
        onDropMediaToLayer(parsed.blake3_hash, row.dropRole);
      }
    } catch {
      // Ignore malformed external drag payload
    }
  }
</script>

<section class="layer-mgr" data-testid="layer-manager">
  <div class="mgr-head">
    <h3>Composition Layers (7-Layer Stack)</h3>
    <div class="master-btns">
      <button class="mini-btn" onclick={onClearAll}>Clear All</button>
      <button
        class="mini-btn blackout"
        class:engaged={scene.blackout}
        onclick={onToggleBlackout}
      >
        {scene.blackout ? "BLACKOUT ON" : "Blackout"}
      </button>
    </div>
  </div>

  <ul class="layer-stack">
    {#each layers as row (row.kind)}
      <li
        class="layer-row"
        class:active={row.active}
        class:drag-over={dragOverLayer === row.kind}
        ondragover={(e) => handleDragOver(e, row)}
        ondragleave={() => {
          if (dragOverLayer === row.kind) dragOverLayer = null;
        }}
        ondrop={(e) => handleDrop(e, row)}
      >
        <span class="status-dot" class:on={row.active}></span>
        <div class="layer-info">
          <div class="layer-title">
            <span class="layer-num">L{row.index}</span>
            <strong>{row.name}</strong>
          </div>
          <span class="layer-summary">{row.summary}</span>
        </div>
        <div class="layer-actions">
          {#if row.kind === "PropsAndOverlays"}
            <button
              class="mini-btn"
              aria-label="Toggle Props & Overlays"
              onclick={onToggleOverlay}
            >
              {row.active ? "Hide" : "Show"}
            </button>
          {:else if row.kind === "VideoInput"}
            <button
              class="mini-btn"
              aria-label="Toggle Video Input"
              onclick={onToggleVideoInput}
            >
              {row.active ? "Off" : "On"}
            </button>
          {/if}
          <button
            class="mini-btn clear-btn"
            aria-label={`Clear ${row.name}`}
            disabled={!row.active}
            onclick={() => onClearLayer(row.kind)}
          >
            Clear
          </button>
        </div>
      </li>
    {/each}
  </ul>
</section>

<style>
  .layer-mgr {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .mgr-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
  }

  .mgr-head h3 {
    margin: 0;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-1);
  }

  .master-btns,
  .layer-actions {
    display: flex;
    gap: 4px;
    align-items: center;
  }

  .layer-stack {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .layer-row {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 7px;
    border-radius: 6px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    transition: border-color 0.12s, background 0.12s;
  }

  .layer-row.active {
    border-color: rgba(59, 130, 246, 0.45);
  }

  .layer-row.drag-over {
    border-color: #38bdf8;
    background: rgba(56, 189, 248, 0.16);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #334155;
    flex-shrink: 0;
  }

  .status-dot.on {
    background: #22c55e;
    box-shadow: 0 0 6px rgba(34, 197, 94, 0.7);
  }

  .layer-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .layer-title {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
  }

  .layer-num {
    font-family: monospace;
    font-size: 10px;
    color: #93c5fd;
  }

  .layer-summary {
    font-size: 10px;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mini-btn {
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 2px 7px;
    font-size: 10px;
    cursor: pointer;
  }

  .mini-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .mini-btn.blackout {
    border-color: rgba(239, 68, 68, 0.5);
    color: #fca5a5;
  }

  .mini-btn.blackout.engaged {
    background: #dc2626;
    color: #fff;
  }
</style>
