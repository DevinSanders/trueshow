<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { RectF, SceneState, Slide, SlideElement, TextAlign } from "./types";

  interface Props {
    slide: Slide;
    onUpdateElement: (slideId: string, element: SlideElement) => Promise<void>;
    onSceneChange?: (next: SceneState) => void;
  }

  let { slide, onUpdateElement, onSceneChange }: Props = $props();

  let selectedElementId = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  let svgEl = $state<SVGSVGElement | null>(null);
  let dragMode = $state<"move" | "resize" | null>(null);
  let dragOrigin = $state<{ px: number; py: number; rect: RectF } | null>(null);
  let snapGuideX = $state<number | null>(null);
  let snapGuideY = $state<number | null>(null);

  const selectedElement = $derived(
    slide.elements.find((e) => e.id === selectedElementId) ?? slide.elements[0] ?? null,
  );

  function newId(): string {
    return typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
      ? crypto.randomUUID()
      : `00000000-0000-4000-8000-${String(Date.now()).slice(-12).padStart(12, "0")}`;
  }

  async function commitElement(updated: SlideElement) {
    saveError = null;
    try {
      await onUpdateElement(slide.id, updated);
    } catch (err) {
      saveError = String(err);
    }
  }

  async function addElement(kind: "text" | "shape") {
    const id = newId();
    const el: SlideElement =
      kind === "text"
        ? {
            id,
            name: `Text Box ${slide.elements.length + 1}`,
            bounds: { x: 240, y: 360, width: 1440, height: 260 },
            opacity: 1,
            z_index: 15 + slide.elements.length,
            kind: {
              TextBox: {
                text: "New Slide Text",
                style: {
                  font_family: "sans-serif",
                  font_size: 72,
                  bold: true,
                  italic: false,
                  line_height: 1.2,
                  color: { r: 1, g: 1, b: 1, a: 1 },
                  stroke_color: { r: 0, g: 0, b: 0, a: 0.9 },
                  stroke_width: 3,
                  shadow_color: { r: 0, g: 0, b: 0, a: 0.7 },
                  shadow_offset_x: 0,
                  shadow_offset_y: 4,
                  shadow_blur: 8,
                  backdrop_color: { r: 0, g: 0, b: 0, a: 0 },
                  backdrop_radius: 14,
                  backdrop_padding: 18,
                  h_align: "Center",
                  v_align: "Middle",
                  auto_shrink: true,
                },
              },
            },
          }
        : {
            id,
            name: `Shape ${slide.elements.length + 1}`,
            bounds: { x: 160, y: 740, width: 1600, height: 200 },
            opacity: 0.85,
            z_index: 5,
            kind: {
              ShapeRect: {
                fill: { r: 0.08, g: 0.14, b: 0.28, a: 0.85 },
                stroke: { r: 0.3, g: 0.6, b: 1, a: 0.9 },
                stroke_width: 3,
                corner_radius: 16,
              },
            },
          };
    selectedElementId = id;
    await commitElement(el);
  }

  async function deleteSelectedElement() {
    if (!selectedElement) return;
    saveError = null;
    try {
      const next = await invoke<SceneState>("delete_slide_element", {
        slideId: slide.id,
        elementId: selectedElement.id,
      });
      selectedElementId = null;
      onSceneChange?.(next);
    } catch (err) {
      saveError = String(err);
    }
  }

  async function updateText(nextText: string) {
    if (!selectedElement || !("TextBox" in selectedElement.kind)) return;
    await commitElement({
      ...selectedElement,
      kind: { TextBox: { ...selectedElement.kind.TextBox, text: nextText } },
    });
  }

  async function updateFontSize(nextSize: number) {
    if (!selectedElement || !("TextBox" in selectedElement.kind)) return;
    await commitElement({
      ...selectedElement,
      kind: {
        TextBox: {
          ...selectedElement.kind.TextBox,
          style: { ...selectedElement.kind.TextBox.style, font_size: nextSize },
        },
      },
    });
  }

  async function updateStrokeWidth(nextStroke: number) {
    if (!selectedElement || !("TextBox" in selectedElement.kind)) return;
    await commitElement({
      ...selectedElement,
      kind: {
        TextBox: {
          ...selectedElement.kind.TextBox,
          style: { ...selectedElement.kind.TextBox.style, stroke_width: nextStroke },
        },
      },
    });
  }

  async function updateAlign(align: TextAlign) {
    if (!selectedElement || !("TextBox" in selectedElement.kind)) return;
    await commitElement({
      ...selectedElement,
      kind: {
        TextBox: {
          ...selectedElement.kind.TextBox,
          style: { ...selectedElement.kind.TextBox.style, h_align: align },
        },
      },
    });
  }

  async function nudgePosition(dx: number, dy: number) {
    if (!selectedElement) return;
    await commitElement({
      ...selectedElement,
      bounds: {
        ...selectedElement.bounds,
        x: Math.max(0, Math.min(1920 - selectedElement.bounds.width, selectedElement.bounds.x + dx)),
        y: Math.max(0, Math.min(1080 - selectedElement.bounds.height, selectedElement.bounds.y + dy)),
      },
    });
  }

  function clientToCanvas(e: PointerEvent): { x: number; y: number } | null {
    if (!svgEl) return null;
    const rect = svgEl.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return null;
    return {
      x: ((e.clientX - rect.left) / rect.width) * 1920,
      y: ((e.clientY - rect.top) / rect.height) * 1080,
    };
  }

  function startPointerOp(e: PointerEvent, el: SlideElement, mode: "move" | "resize") {
    e.stopPropagation();
    selectedElementId = el.id;
    const pt = clientToCanvas(e);
    if (!pt) return;
    dragMode = mode;
    dragOrigin = { px: pt.x, py: pt.y, rect: { ...el.bounds } };
  }

  function onCanvasPointerMove(e: PointerEvent) {
    if (!dragMode || !dragOrigin || !selectedElement) return;
    const pt = clientToCanvas(e);
    if (!pt) return;
    const dx = pt.x - dragOrigin.px;
    const dy = pt.y - dragOrigin.py;
    const nextBounds: RectF = { ...dragOrigin.rect };
    if (dragMode === "move") {
      nextBounds.x = Math.max(0, Math.min(1920 - nextBounds.width, Math.round(dragOrigin.rect.x + dx)));
      nextBounds.y = Math.max(0, Math.min(1080 - nextBounds.height, Math.round(dragOrigin.rect.y + dy)));
      const cx = nextBounds.x + nextBounds.width / 2;
      const cy = nextBounds.y + nextBounds.height / 2;
      snapGuideX = Math.abs(cx - 960) < 24 ? 960 : null;
      snapGuideY = Math.abs(cy - 540) < 24 ? 540 : null;
      if (snapGuideX !== null) nextBounds.x = Math.round(960 - nextBounds.width / 2);
      if (snapGuideY !== null) nextBounds.y = Math.round(540 - nextBounds.height / 2);
    } else {
      nextBounds.width = Math.max(120, Math.min(1920 - nextBounds.x, Math.round(dragOrigin.rect.width + dx)));
      nextBounds.height = Math.max(60, Math.min(1080 - nextBounds.y, Math.round(dragOrigin.rect.height + dy)));
    }
    void commitElement({ ...selectedElement, bounds: nextBounds });
  }

  function endPointerOp() {
    dragMode = null;
    dragOrigin = null;
    snapGuideX = null;
    snapGuideY = null;
  }
</script>

<div class="editor-grid" data-testid="slide-editor">
  <div class="stage-canvas">
    <div class="canvas-meta">
      <span>WYSIWYG Drag & Resize Canvas (1920×1080 · Snap Guides)</span>
      <div class="toolbar">
        <button class="mini-btn" onclick={() => void addElement("text")}>+ Text</button>
        <button class="mini-btn" onclick={() => void addElement("shape")}>+ Shape</button>
        <button class="mini-btn danger" disabled={!selectedElement} onclick={() => void deleteSelectedElement()}>
          Delete Element
        </button>
      </div>
    </div>
    <svg
      bind:this={svgEl}
      class="gizmo-svg"
      viewBox="0 0 1920 1080"
      role="application"
      aria-label="Slide element bounding boxes"
      onpointermove={onCanvasPointerMove}
      onpointerup={endPointerOp}
      onpointerleave={endPointerOp}
    >
      <rect x="0" y="0" width="1920" height="1080" fill="#0d1320" />
      <rect
        x="96"
        y="54"
        width="1728"
        height="972"
        fill="none"
        stroke="#27364f"
        stroke-dasharray="14 10"
        stroke-width="3"
      />
      {#if snapGuideX !== null}
        <line x1={snapGuideX} y1="0" x2={snapGuideX} y2="1080" stroke="#38bdf8" stroke-width="3" stroke-dasharray="10 8" />
      {/if}
      {#if snapGuideY !== null}
        <line x1="0" y1={snapGuideY} x2="1920" y2={snapGuideY} stroke="#38bdf8" stroke-width="3" stroke-dasharray="10 8" />
      {/if}
      {#each slide.elements as el (el.id)}
        {@const isSel = selectedElement?.id === el.id}
        <g
          role="button"
          tabindex="0"
          aria-label={`Select ${el.name}`}
          onclick={() => (selectedElementId = el.id)}
          onpointerdown={(e) => startPointerOp(e, el, "move")}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") selectedElementId = el.id;
          }}
        >
          <rect
            x={el.bounds.x}
            y={el.bounds.y}
            width={el.bounds.width}
            height={el.bounds.height}
            fill={isSel ? "rgba(59,130,246,0.16)" : "rgba(255,255,255,0.04)"}
            stroke={isSel ? "#3b82f6" : "#64748b"}
            stroke-width={isSel ? 5 : 2}
          />
          <text
            x={el.bounds.x + 16}
            y={el.bounds.y + 42}
            fill="#cbd5e1"
            font-size="32"
            font-family="sans-serif"
          >
            {el.name}
          </text>
          {#if isSel}
            <rect
              x={el.bounds.x + el.bounds.width - 28}
              y={el.bounds.y + el.bounds.height - 28}
              width="28"
              height="28"
              fill="#38bdf8"
              role="button"
              tabindex="0"
              aria-label={`Resize ${el.name}`}
              onpointerdown={(e) => startPointerOp(e, el, "resize")}
            />
          {/if}
        </g>
      {/each}
    </svg>
  </div>

  <div class="inspector">
    <h3>Element Inspector · {slide.label}</h3>
    {#if saveError}
      <div class="err" role="alert">{saveError}</div>
    {/if}

    {#if selectedElement && "TextBox" in selectedElement.kind}
      {@const tb = selectedElement.kind.TextBox}
      <label class="field">
        <span>Lyric / Scripture Text</span>
        <textarea
          aria-label="Lyric Text"
          rows="5"
          value={tb.text}
          oninput={(e) => updateText(e.currentTarget.value)}
        ></textarea>
      </label>

      <div class="field">
        <span>Insert Live Token</span>
        <div class="btn-row">
          <button class="mini-btn" onclick={() => void updateText(`${tb.text} {{timer}}`)}>Timer</button>
          <button class="mini-btn" onclick={() => void updateText(`${tb.text} {{clock}}`)}>Clock</button>
          <button class="mini-btn" onclick={() => void updateText(`${tb.text} {{var:TeamA}}`)}>Var</button>
        </div>
      </div>

      <label class="field">
        <span>Font Size ({tb.style.font_size} px)</span>
        <input
          aria-label="Font Size"
          type="range"
          min="28"
          max="140"
          step="2"
          value={tb.style.font_size}
          oninput={(e) => updateFontSize(Number(e.currentTarget.value))}
        />
      </label>

      <label class="field">
        <span>Outline Stroke ({tb.style.stroke_width} px)</span>
        <input
          aria-label="Stroke Width"
          type="range"
          min="0"
          max="10"
          step="0.5"
          value={tb.style.stroke_width}
          oninput={(e) => updateStrokeWidth(Number(e.currentTarget.value))}
        />
      </label>

      <div class="field">
        <span>Alignment</span>
        <div class="btn-row">
          {#each ["Left", "Center", "Right"] as align (align)}
            <button
              class="mini-btn"
              class:active={tb.style.h_align === align}
              onclick={() => updateAlign(align as TextAlign)}
            >
              {align}
            </button>
          {/each}
        </div>
      </div>

      <div class="field">
        <span>Position ({Math.round(selectedElement.bounds.x)}, {Math.round(selectedElement.bounds.y)})</span>
        <div class="btn-row">
          <button class="mini-btn" onclick={() => nudgePosition(-20, 0)}>← X</button>
          <button class="mini-btn" onclick={() => nudgePosition(20, 0)}>X →</button>
          <button class="mini-btn" onclick={() => nudgePosition(0, -20)}>↑ Y</button>
          <button class="mini-btn" onclick={() => nudgePosition(0, 20)}>Y ↓</button>
        </div>
      </div>
    {:else}
      <p class="muted">Select a text element to edit its typography and geometry, or drag shapes directly on the canvas.</p>
    {/if}
  </div>
</div>

<style>
  .editor-grid {
    display: grid;
    grid-template-columns: 1fr 300px;
    gap: 14px;
    flex: 1;
    min-height: 0;
  }

  .stage-canvas {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px;
  }

  .canvas-meta {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-1);
  }

  .toolbar {
    display: flex;
    gap: 6px;
  }

  .gizmo-svg {
    width: 100%;
    aspect-ratio: 16 / 9;
    border-radius: 6px;
    border: 1px solid var(--border);
    cursor: move;
    touch-action: none;
  }

  .inspector {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
  }

  .inspector h3 {
    margin: 0;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-1);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 12px;
    color: var(--text-1);
  }

  .field textarea {
    width: 100%;
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 8px;
    font: inherit;
    font-size: 13px;
    resize: vertical;
  }

  .btn-row {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .mini-btn {
    padding: 5px 8px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 11px;
    font-weight: 600;
  }

  .mini-btn.active {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .mini-btn.danger {
    background: rgba(239, 68, 68, 0.16);
    border-color: rgba(239, 68, 68, 0.45);
    color: #fecaca;
  }

  .err {
    background: rgba(239, 68, 68, 0.18);
    color: #fecaca;
    padding: 6px 8px;
    border-radius: 5px;
    font-size: 11px;
  }

  .muted {
    color: var(--text-1);
    font-size: 12px;
  }
</style>
