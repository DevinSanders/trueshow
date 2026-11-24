<script lang="ts">
  import type { Slide, SlideElement, TextAlign } from "./types";

  interface Props {
    slide: Slide;
    onUpdateElement: (slideId: string, element: SlideElement) => Promise<void>;
  }

  let { slide, onUpdateElement }: Props = $props();

  let selectedElementId = $state<string | null>(null);
  let saveError = $state<string | null>(null);

  const selectedElement = $derived(
    slide.elements.find((e) => e.id === selectedElementId) ?? slide.elements[0] ?? null,
  );

  async function commitElement(updated: SlideElement) {
    saveError = null;
    try {
      await onUpdateElement(slide.id, updated);
    } catch (err) {
      saveError = String(err);
    }
  }

  async function updateText(nextText: string) {
    if (!selectedElement || !("TextBox" in selectedElement.kind)) return;
    await commitElement({
      ...selectedElement,
      kind: {
        TextBox: {
          ...selectedElement.kind.TextBox,
          text: nextText,
        },
      },
    });
  }

  async function updateFontSize(nextSize: number) {
    if (!selectedElement || !("TextBox" in selectedElement.kind)) return;
    await commitElement({
      ...selectedElement,
      kind: {
        TextBox: {
          ...selectedElement.kind.TextBox,
          style: {
            ...selectedElement.kind.TextBox.style,
            font_size: nextSize,
          },
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
          style: {
            ...selectedElement.kind.TextBox.style,
            stroke_width: nextStroke,
          },
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
          style: {
            ...selectedElement.kind.TextBox.style,
            h_align: align,
          },
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
</script>

<div class="editor-grid" data-testid="slide-editor">
  <div class="stage-canvas">
    <div class="canvas-meta">
      <span>Zero-Drift Canvas Gizmo Overlay (1920×1080 Virtual Space)</span>
      <span>Slide: <strong>{slide.label}</strong></span>
    </div>
    <svg class="gizmo-svg" viewBox="0 0 1920 1080" aria-label="Slide element bounding boxes">
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
      {#each slide.elements as el (el.id)}
        {@const isSel = selectedElement?.id === el.id}
        <g
          role="button"
          tabindex="0"
          aria-label={`Select ${el.name}`}
          onclick={() => (selectedElementId = el.id)}
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
        </g>
      {/each}
    </svg>
  </div>

  <div class="inspector">
    <h3>Element Inspector</h3>
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
      <p class="muted">Select a text element to edit its typography and geometry.</p>
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
    font-size: 12px;
    color: var(--text-1);
  }

  .gizmo-svg {
    width: 100%;
    aspect-ratio: 16 / 9;
    border-radius: 6px;
    border: 1px solid var(--border);
    cursor: pointer;
  }

  .inspector {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
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
    gap: 6px;
    font-size: 12px;
    color: var(--text-1);
  }

  textarea {
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
    font: inherit;
    resize: vertical;
  }

  .btn-row {
    display: flex;
    gap: 6px;
  }

  .mini-btn {
    flex: 1;
    padding: 5px 8px;
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    font-size: 11px;
  }

  .mini-btn.active {
    background: var(--accent);
    border-color: var(--accent);
  }

  .err {
    padding: 6px 8px;
    border-radius: 5px;
    background: rgba(239, 68, 68, 0.18);
    color: #fecaca;
    font-size: 11px;
  }

  .muted {
    font-size: 12px;
    color: var(--text-1);
  }
</style>
