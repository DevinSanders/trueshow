<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { RectF, SceneState, StageLayoutConfig } from "./types";

  interface Props {
    scene: SceneState;
    onSceneChange: (next: SceneState) => void;
    onError?: (msg: string) => void;
  }

  let { scene, onSceneChange, onError }: Props = $props();

  const stageOutputs = $derived(scene.outputs.filter((o) => o.kind === "Stage"));
  let selectedOutputId = $state("");
  let layout = $state<StageLayoutConfig>({
    name: "Band Confidence (Default)",
    show_current_slide: true,
    show_next_slide: true,
    show_chords: true,
    show_clock: true,
    show_timer: true,
    show_stage_message: true,
    current_rect: { x: 120, y: 230, width: 1680, height: 430 },
    next_rect: { x: 120, y: 760, width: 1680, height: 190 },
    chords_rect: { x: 120, y: 150, width: 1680, height: 84 },
    timer_rect: { x: 1380, y: 12, width: 480, height: 96 },
    message_rect: { x: 160, y: 930, width: 1600, height: 120 },
  });
  let notice = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  const effectiveOutputId = $derived(selectedOutputId || (stageOutputs[0]?.id ?? ""));

  function applyPreset(preset: "band" | "pastor" | "minimal") {
    if (preset === "pastor") {
      layout = {
        ...layout,
        name: "Pastor Big Clock & Notes",
        show_chords: false,
        show_timer: true,
        show_next_slide: true,
        timer_rect: { x: 460, y: 40, width: 1000, height: 220 },
        current_rect: { x: 120, y: 300, width: 1680, height: 420 },
        next_rect: { x: 120, y: 760, width: 1680, height: 150 },
      };
    } else if (preset === "minimal") {
      layout = {
        ...layout,
        name: "Minimal Lyrics Only",
        show_chords: false,
        show_next_slide: false,
        show_timer: false,
        current_rect: { x: 100, y: 160, width: 1720, height: 760 },
      };
    } else {
      layout = {
        name: "Band Confidence (Default)",
        show_current_slide: true,
        show_next_slide: true,
        show_chords: true,
        show_clock: true,
        show_timer: true,
        show_stage_message: true,
        current_rect: { x: 120, y: 230, width: 1680, height: 430 },
        next_rect: { x: 120, y: 760, width: 1680, height: 190 },
        chords_rect: { x: 120, y: 150, width: 1680, height: 84 },
        timer_rect: { x: 1380, y: 12, width: 480, height: 96 },
        message_rect: { x: 160, y: 930, width: 1600, height: 120 },
      };
    }
  }

  function boxStyle(r: RectF): string {
    return `left:${(r.x / 1920) * 100}%;top:${(r.y / 1080) * 100}%;width:${(r.width / 1920) * 100}%;height:${(r.height / 1080) * 100}%;`;
  }

  async function saveStageLayout() {
    errorMsg = null;
    notice = null;
    if (!effectiveOutputId) {
      errorMsg = "Create a Stage Display in Screens first before applying a Stage Layout.";
      return;
    }
    try {
      const next = await invoke<SceneState>("set_output_stage_layout", {
        outputId: effectiveOutputId,
        layout,
      });
      onSceneChange(next);
      notice = `Applied "${layout.name}" live to Stage Display.`;
    } catch (err) {
      errorMsg = `Failed to apply stage layout: ${String(err)}`;
      onError?.(errorMsg);
    }
  }
</script>

<section class="sd-panel" data-testid="stage-designer-panel">
  <header class="sd-head">
    <div>
      <h2>Visual Stage Confidence Display Designer</h2>
      <p class="sub">
        Customize confidence monitor zones (Current Slide, Next Slide, Chords, Countdown Timer, Urgent Message) per Stage output.
      </p>
    </div>
    <div class="presets">
      <button class="btn sm" onclick={() => applyPreset("band")}>Band w/ Chords</button>
      <button class="btn sm" onclick={() => applyPreset("pastor")}>Pastor Big Clock</button>
      <button class="btn sm" onclick={() => applyPreset("minimal")}>Minimal Lyrics</button>
    </div>
  </header>

  {#if errorMsg}
    <div class="banner err" role="alert">{errorMsg}</div>
  {/if}
  {#if notice}
    <div class="banner ok" role="status">{notice}</div>
  {/if}

  <div class="two-col">
    <div class="card">
      <div class="stage-preview" aria-label="Stage Display 16:9 Layout Preview">
        {#if layout.show_timer}
          <div class="zone timer" style={boxStyle(layout.timer_rect)}>TIMER · 05:00</div>
        {/if}
        {#if layout.show_chords}
          <div class="zone chords" style={boxStyle(layout.chords_rect)}>[G]Amazing [C]grace!</div>
        {/if}
        {#if layout.show_current_slide}
          <div class="zone curr" style={boxStyle(layout.current_rect)}>CURRENT SLIDE LYRICS</div>
        {/if}
        {#if layout.show_next_slide}
          <div class="zone next" style={boxStyle(layout.next_rect)}>NEXT: That saved a wretch like me</div>
        {/if}
        {#if layout.show_stage_message}
          <div class="zone msg" style={boxStyle(layout.message_rect)}>URGENT STAGE MESSAGE</div>
        {/if}
      </div>
    </div>

    <div class="card">
      <label>
        Target Stage Display
        <select aria-label="Target Stage Display" bind:value={selectedOutputId}>
          {#each stageOutputs as out (out.id)}
            <option value={out.id}>{out.name}</option>
          {:else}
            <option value="">No Stage Display configured</option>
          {/each}
        </select>
      </label>

      <label>
        Layout Name
        <input aria-label="Stage Layout Name" bind:value={layout.name} />
      </label>

      <div class="checks">
        <label class="chk"><input type="checkbox" bind:checked={layout.show_current_slide} /> Current Slide</label>
        <label class="chk"><input type="checkbox" bind:checked={layout.show_next_slide} /> Next Slide</label>
        <label class="chk"><input type="checkbox" bind:checked={layout.show_chords} /> Chords Band</label>
        <label class="chk"><input type="checkbox" bind:checked={layout.show_timer} /> Countdown Timer</label>
        <label class="chk"><input type="checkbox" bind:checked={layout.show_stage_message} /> Urgent Message</label>
      </div>

      <div class="coords">
        <label>Current Y <input aria-label="Current Zone Y" type="number" bind:value={layout.current_rect.y} /></label>
        <label>Current H <input aria-label="Current Zone Height" type="number" bind:value={layout.current_rect.height} /></label>
        <label>Timer X <input aria-label="Timer Zone X" type="number" bind:value={layout.timer_rect.x} /></label>
        <label>Timer W <input aria-label="Timer Zone Width" type="number" bind:value={layout.timer_rect.width} /></label>
      </div>

      <button class="btn primary" onclick={() => void saveStageLayout()}>Apply Stage Layout Live</button>
    </div>
  </div>
</section>

<style>
  .sd-panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    flex: 1;
    overflow-y: auto;
  }

  .sd-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  h2 {
    margin: 0;
    font-size: 14px;
  }

  .sub {
    margin: 3px 0 0;
    font-size: 11px;
    color: var(--text-1);
  }

  .presets,
  .checks {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .two-col {
    display: grid;
    grid-template-columns: 1.4fr 1fr;
    gap: 12px;
  }

  .card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .stage-preview {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    background: #06080f;
    border: 1px solid #2b354b;
    border-radius: 6px;
    overflow: hidden;
  }

  .zone {
    position: absolute;
    border: 1px solid;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    padding: 2px 6px;
    box-sizing: border-box;
    text-align: center;
  }

  .zone.timer {
    border-color: #34d399;
    background: rgba(16, 185, 129, 0.22);
    color: #6ee7b7;
  }

  .zone.chords {
    border-color: #fbbf24;
    background: rgba(245, 158, 11, 0.2);
    color: #fde68a;
  }

  .zone.curr {
    border-color: #60a5fa;
    background: rgba(37, 99, 235, 0.22);
    color: #fff;
  }

  .zone.next {
    border-color: #94a3b8;
    background: rgba(100, 116, 139, 0.22);
    color: #cbd5e1;
  }

  .zone.msg {
    border-color: #f87171;
    background: rgba(220, 38, 38, 0.28);
    color: #fecaca;
  }

  .coords {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--text-1);
  }

  .chk {
    flex-direction: row;
    align-items: center;
    gap: 5px;
    color: var(--text-0);
  }

  input:not([type="checkbox"]),
  select {
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
    font-size: 12px;
  }

  .banner {
    padding: 7px 10px;
    border-radius: 6px;
    font-size: 12px;
  }

  .banner.err {
    background: rgba(239, 68, 68, 0.16);
    color: #fecaca;
  }

  .banner.ok {
    background: rgba(16, 185, 129, 0.16);
    color: #a7f3d0;
  }

  .btn {
    padding: 6px 11px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 12px;
    font-weight: 600;
  }

  .btn.sm {
    padding: 4px 9px;
    font-size: 11px;
  }

  .btn.primary {
    background: #2563eb;
    border-color: #3b82f6;
    color: #fff;
  }
</style>
