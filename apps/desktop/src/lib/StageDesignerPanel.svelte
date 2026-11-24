<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { EngineSnapshot, RectF, SceneState, StageDisplayMode, StageLayoutConfig } from "./types";

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
    mode: "Confidence",
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
    chord_lookahead_sections: 2,
    chord_extra_lines: 10,
    transpose_semitones: 0,
    sheet_two_page_spread: true,
    sheet_dark_invert: false,
  });
  let notice = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  const effectiveOutputId = $derived(selectedOutputId || (stageOutputs[0]?.id ?? ""));
  const effectiveMode = $derived<StageDisplayMode>(layout.mode ?? "Confidence");
  const sheetPages = $derived(scene.active_sheet_music?.pages ?? []);

  function syncSelectedOutputLayout(outId: string) {
    selectedOutputId = outId;
    const hit = stageOutputs.find((o) => o.id === outId);
    if (hit?.stage_layout) {
      layout = {
        ...layout,
        ...hit.stage_layout,
        mode: hit.stage_layout.mode ?? "Confidence",
        chord_lookahead_sections: hit.stage_layout.chord_lookahead_sections ?? 2,
        chord_extra_lines: hit.stage_layout.chord_extra_lines ?? 10,
        transpose_semitones: hit.stage_layout.transpose_semitones ?? 0,
        sheet_two_page_spread: hit.stage_layout.sheet_two_page_spread ?? true,
        sheet_dark_invert: hit.stage_layout.sheet_dark_invert ?? false,
      };
    }
  }

  function applyPreset(preset: "band" | "pastor" | "minimal" | "chords" | "sheet") {
    if (preset === "chords") {
      layout = {
        ...layout,
        name: "Musician Chord Chart (Full + Lookahead)",
        mode: "ChordChart",
        show_chords: true,
        show_timer: true,
        show_stage_message: true,
        chord_lookahead_sections: 2,
        chord_extra_lines: 10,
        transpose_semitones: 0,
      };
    } else if (preset === "sheet") {
      layout = {
        ...layout,
        name: "Musician Sheet Music (Full Clef 2-Page)",
        mode: "SheetMusic",
        show_timer: true,
        show_stage_message: true,
        sheet_two_page_spread: true,
        sheet_dark_invert: false,
      };
    } else if (preset === "pastor") {
      layout = {
        ...layout,
        name: "Pastor Big Clock & Notes",
        mode: "Confidence",
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
        mode: "Confidence",
        show_chords: false,
        show_next_slide: false,
        show_timer: false,
        current_rect: { x: 100, y: 160, width: 1720, height: 760 },
      };
    } else {
      layout = {
        ...layout,
        name: "Band Confidence (Default)",
        mode: "Confidence",
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

  async function createDedicatedMusicianScreen(mode: "ChordChart" | "SheetMusic") {
    errorMsg = null;
    notice = null;
    try {
      const label =
        mode === "SheetMusic"
          ? `Musician Sheet Music (${stageOutputs.length + 1})`
          : `Musician Chord Chart (${stageOutputs.length + 1})`;
      const snap = await invoke<EngineSnapshot>("upsert_output_screen", {
        outputId: null,
        name: label,
        kind: "Stage",
        monitorIndex: null,
        fullscreen: false,
        placement: "Windowed",
        width: 1280,
        height: 720,
        lookId: null,
      });
      const created = snap.state.outputs.find((o) => o.kind === "Stage" && o.name === label);
      onSceneChange(snap.state);
      applyPreset(mode === "SheetMusic" ? "sheet" : "chords");
      if (created) {
        selectedOutputId = created.id;
        const next = await invoke<SceneState>("set_output_stage_layout", {
          outputId: created.id,
          layout,
        });
        onSceneChange(next);
      }
      notice = `Created separate synced Stage Display "${label}".`;
    } catch (err) {
      errorMsg = `Failed to create Musician Stage Display: ${String(err)}`;
      onError?.(errorMsg);
    }
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
      <h2>Visual Stage & Musician Display Designer</h2>
      <p class="sub">
        Run separate synced Stage Displays for Vocalists (Confidence) and Musicians (Full Chord Charts w/ Lookahead or Full-Clef Sheet Music).
      </p>
    </div>
    <div class="presets">
      <button class="btn sm" onclick={() => applyPreset("band")}>Band w/ Chords</button>
      <button class="btn sm" onclick={() => applyPreset("pastor")}>Pastor Big Clock</button>
      <button class="btn sm" onclick={() => applyPreset("minimal")}>Minimal Lyrics</button>
      <button class="btn sm accent" onclick={() => applyPreset("chords")}>Musician Chord Chart</button>
      <button class="btn sm accent" onclick={() => applyPreset("sheet")}>Musician Sheet Music</button>
      <button class="btn sm" onclick={() => void createDedicatedMusicianScreen("ChordChart")}>
        + Separate Musician Screen
      </button>
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
        {#if effectiveMode === "ChordChart"}
          <div class="musician-preview chords-grid">
            <div class="mp-head">
              <span>{scene.active_chord_chart?.title ?? scene.active_presentation_title ?? "Amazing Grace"} · Key {scene.active_chord_chart?.key_signature ?? "G"} ({(layout.transpose_semitones ?? 0) >= 0 ? `+${layout.transpose_semitones ?? 0}` : layout.transpose_semitones})</span>
              {#if layout.show_timer}<span class="mp-timer">05:00</span>{/if}
            </div>
            <div class="mp-cols">
              <div class="mp-active">
                <div class="mp-tag">▶ NOW: FULL ACTIVE SECTION</div>
                <div class="mp-chord">G          C          G</div>
                <div class="mp-lyric">Amazing grace! How sweet the sound</div>
                <div class="mp-chord">G          Em         D</div>
                <div class="mp-lyric">That saved a wretch like me!</div>
                <div class="mp-chord">G   G7     C          G</div>
                <div class="mp-lyric">I once was lost, but now am found</div>
              </div>
              <div class="mp-lookahead">
                <div class="mp-tag">NEXT +{layout.chord_lookahead_sections ?? 2} SECTIONS ({layout.chord_extra_lines ?? 10} EXTRA LINES)</div>
                <div class="mp-chord">C     G/B     D/F#   G</div>
                <div class="mp-lyric">My chains are gone, I've been set free</div>
                <div class="mp-tag">VERSE 2</div>
                <div class="mp-chord">G          C          G</div>
                <div class="mp-lyric">'Twas grace that taught my heart to fear</div>
              </div>
            </div>
          </div>
        {:else if effectiveMode === "SheetMusic"}
          <div class="musician-preview sheet-grid">
            <div class="mp-head">
              <span>{scene.active_sheet_music?.title ?? "Full-Clef Grand Staff Score"} · {sheetPages.length || 2} pages synced</span>
              {#if layout.show_timer}<span class="mp-timer">05:00</span>{/if}
            </div>
            <div class="mp-sheet-pages" class:single={!layout.sheet_two_page_spread}>
              <div class="mp-paper" class:inverted={layout.sheet_dark_invert}>
                <strong>▶ PAGE 1 (TREBLE + BASS CLEF)</strong>
                <div class="staff-bar">𝄞 ═══════════════</div>
                <div class="staff-bar">𝄢 ═══════════════</div>
                <div class="staff-bar">𝄞 ═══════════════</div>
                <div class="staff-bar">𝄢 ═══════════════</div>
              </div>
              {#if layout.sheet_two_page_spread}
                <div class="mp-paper dim" class:inverted={layout.sheet_dark_invert}>
                  <strong>NEXT: PAGE 2 (AUTO-TURN)</strong>
                  <div class="staff-bar">𝄞 ═══════════════</div>
                  <div class="staff-bar">𝄢 ═══════════════</div>
                  <div class="staff-bar">𝄞 ═══════════════</div>
                  <div class="staff-bar">𝄢 ═══════════════</div>
                </div>
              {/if}
            </div>
          </div>
        {:else}
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
        {/if}
      </div>
    </div>

    <div class="card">
      <label>
        Target Stage Display
        <select
          aria-label="Target Stage Display"
          value={effectiveOutputId}
          onchange={(e) => syncSelectedOutputLayout(e.currentTarget.value)}
        >
          {#each stageOutputs as out (out.id)}
            <option value={out.id}>{out.name}</option>
          {:else}
            <option value="">No Stage Display configured</option>
          {/each}
        </select>
      </label>

      <label>
        Display Mode (Vocalists vs. Musicians)
        <select aria-label="Stage Display Mode" bind:value={layout.mode}>
          <option value="Confidence">Confidence Monitor (Vocalists / Pastor)</option>
          <option value="ChordChart">Musician Chord Chart (Full Section + Extra Lines)</option>
          <option value="SheetMusic">Musician Sheet Music (Full Clef / Auto Page Turn)</option>
        </select>
      </label>

      <label>
        Layout Name
        <input aria-label="Stage Layout Name" bind:value={layout.name} />
      </label>

      {#if effectiveMode === "ChordChart"}
        <div class="coords">
          <label>
            Lookahead Sections (1–4)
            <input
              aria-label="Lookahead Sections"
              type="number"
              min="1"
              max="4"
              bind:value={layout.chord_lookahead_sections}
            />
          </label>
          <label>
            Extra Lookahead Lines
            <input
              aria-label="Extra Lookahead Lines"
              type="number"
              min="4"
              max="24"
              bind:value={layout.chord_extra_lines}
            />
          </label>
          <label>
            Live Transpose (Semitones)
            <input
              aria-label="Transpose Semitones"
              type="number"
              min="-6"
              max="6"
              bind:value={layout.transpose_semitones}
            />
          </label>
        </div>
        <div class="checks">
          <label class="chk"><input type="checkbox" bind:checked={layout.show_timer} /> Countdown Timer</label>
          <label class="chk"><input type="checkbox" bind:checked={layout.show_stage_message} /> Urgent Message</label>
        </div>
      {:else if effectiveMode === "SheetMusic"}
        <div class="checks">
          <label class="chk"><input type="checkbox" bind:checked={layout.sheet_two_page_spread} /> 2-Page Side-by-Side Spread</label>
          <label class="chk"><input type="checkbox" bind:checked={layout.sheet_dark_invert} /> Dark Stage Paper Invert</label>
          <label class="chk"><input type="checkbox" bind:checked={layout.show_timer} /> Countdown Timer</label>
          <label class="chk"><input type="checkbox" bind:checked={layout.show_stage_message} /> Urgent Message</label>
        </div>
      {:else}
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
      {/if}

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

  .btn.accent {
    background: rgba(14, 165, 233, 0.2);
    border-color: rgba(56, 189, 248, 0.6);
    color: #bae6fd;
  }

  .musician-preview {
    width: 100%;
    height: 100%;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    box-sizing: border-box;
    font-size: 9px;
  }

  .mp-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 8px;
    background: #0f172a;
    border-radius: 4px;
    font-weight: 700;
    color: #e2e8f0;
  }

  .mp-timer {
    color: #34d399;
  }

  .mp-cols {
    flex: 1;
    display: grid;
    grid-template-columns: 1.15fr 1fr;
    gap: 6px;
    min-height: 0;
  }

  .mp-active,
  .mp-lookahead {
    background: #0d1424;
    border: 1px solid #334155;
    border-radius: 5px;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow: hidden;
  }

  .mp-active {
    border-color: #38bdf8;
    background: #0f1c33;
  }

  .mp-tag {
    font-size: 8px;
    font-weight: 800;
    color: #38bdf8;
    margin-bottom: 2px;
  }

  .mp-chord {
    font-family: ui-monospace, monospace;
    color: #fbbf24;
    font-weight: 700;
    white-space: pre;
  }

  .mp-lyric {
    font-family: ui-monospace, monospace;
    color: #f8fafc;
    margin-bottom: 3px;
  }

  .mp-sheet-pages {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    min-height: 0;
  }

  .mp-sheet-pages.single {
    grid-template-columns: 1fr;
    padding: 0 18%;
  }

  .mp-paper {
    background: #f8fafc;
    color: #0f172a;
    border: 2px solid #38bdf8;
    border-radius: 5px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    font-size: 9px;
  }

  .mp-paper.dim {
    border-color: #64748b;
    opacity: 0.9;
  }

  .mp-paper.inverted {
    background: #0f172a;
    color: #e2e8f0;
  }

  .staff-bar {
    font-family: serif;
    letter-spacing: 0.05em;
    opacity: 0.85;
  }
</style>
