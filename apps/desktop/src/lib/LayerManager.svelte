<script lang="ts">
  import { allowDrop, readDroppedMedia } from "./dragStore";
  import type { LayerKind, SceneState } from "./types";

  interface Props {
    scene: SceneState;
    onClearLayer: (layer: LayerKind) => void;
    onClearAll: () => void;
    onToggleBlackout: () => void;
    onToggleOverlay: () => void;
    onToggleVideoInput: () => void;
    onDropMediaToLayer?: (
      blake3Hash: string,
      role: "background" | "foreground" | "announcement",
    ) => void;
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
      index: 0,
      kind: "VideoInput",
      name: "Video Input",
      active: scene.video_input_active,
      summary: scene.video_input_active ? "Live Input Active" : "Off",
    },
    {
      index: 1,
      kind: "MediaBackground",
      name: "Media / Background",
      active: scene.media_layer_active && Boolean(scene.active_background),
      summary:
        scene.media_layer_active && scene.active_background
          ? scene.active_background.label
          : "Drop BG here",
      dropRole: "background",
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
            : "Drop FG here",
      dropRole: "foreground",
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
          : scene.announcement_text || "Empty",
      dropRole: "announcement",
    },
    {
      index: 4,
      kind: "PropsAndOverlays",
      name: "Props & Overlays",
      active: scene.overlay_layer_active,
      summary: scene.overlay_layer_active ? "Overlay Bug ON" : "Off",
    },
    {
      index: 5,
      kind: "Masks",
      name: "Masks",
      active: scene.mask_style !== "None",
      summary: scene.mask_style !== "None" ? scene.mask_style : "None",
    },
    {
      index: 6,
      kind: "MessagesAndTimers",
      name: "Messages & Timers",
      active: Boolean(scene.stage_message || scene.timer_readout),
      summary:
        [scene.timer_readout, scene.stage_message].filter(Boolean).join(" · ") || "Empty",
    },
  ]);

  function handleDragOver(event: DragEvent, row: LayerRow) {
    if (!row.dropRole) return;
    if (allowDrop(event, ["media"], "copy")) {
      dragOverLayer = row.kind;
    }
  }

  function handleDrop(event: DragEvent, row: LayerRow) {
    dragOverLayer = null;
    if (!row.dropRole || !onDropMediaToLayer) return;
    event.preventDefault();
    const media = readDroppedMedia(event);
    if (media?.blake3_hash) {
      onDropMediaToLayer(media.blake3_hash, row.dropRole);
    }
  }
</script>

<section class="layer-dock" data-testid="layer-manager">
  <div class="mgr-head">
    <h3>Composition Layers (L0–L6)</h3>
    <div class="master-btns">
      <button class="mini-btn" onclick={onClearAll}>Clear All</button>
      <button
        class="mini-btn blackout"
        class:engaged={scene.blackout}
        onclick={onToggleBlackout}
      >
        {scene.blackout ? "BLACKOUT ACTIVE" : "BLACKOUT"}
      </button>
    </div>
  </div>

  <ul class="layer-strip">
    {#each layers as row (row.kind)}
      <li
        class="layer-chip"
        class:active={row.active}
        class:drag-over={dragOverLayer === row.kind}
        ondragenter={(e) => handleDragOver(e, row)}
        ondragover={(e) => handleDragOver(e, row)}
        ondragleave={() => {
          if (dragOverLayer === row.kind) dragOverLayer = null;
        }}
        ondrop={(e) => handleDrop(e, row)}
      >
        <div class="chip-top">
          <span class="status-dot" class:on={row.active}></span>
          <span class="layer-num">L{row.index}</span>
          <strong class="layer-name">{row.name}</strong>
        </div>
        <div class="layer-summary" title={row.summary}>{row.summary}</div>
        <div class="chip-actions">
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
  .layer-dock {
    background: #0d121c;
    border-top: 1px solid var(--border);
    padding: 7px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex-shrink: 0;
  }

  .mgr-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .mgr-head h3 {
    margin: 0;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-1);
  }

  .master-btns,
  .chip-actions {
    display: flex;
    gap: 4px;
    align-items: center;
  }

  .layer-strip {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(7, minmax(110px, 1fr));
    gap: 6px;
    overflow-x: auto;
  }

  .layer-chip {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 7px;
    border-radius: 6px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    min-width: 0;
  }

  .layer-chip.active {
    border-color: rgba(59, 130, 246, 0.5);
  }

  .layer-chip.drag-over {
    outline: 1px dashed #38bdf8;
    background: rgba(56, 189, 248, 0.16);
  }

  .chip-top {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
  }

  .layer-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #334155;
    flex-shrink: 0;
  }

  .status-dot.on {
    background: #22c55e;
    box-shadow: 0 0 6px rgba(34, 197, 94, 0.7);
  }

  .layer-num {
    font-family: monospace;
    font-size: 9px;
    color: #93c5fd;
    flex-shrink: 0;
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
    border-radius: 4px;
    padding: 2px 6px;
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
