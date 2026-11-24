<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type {
    DisplayInfo,
    EngineSnapshot,
    LookConfig,
    MonitorRegion,
    OutputConfig,
  } from "./types";

  interface Props {
    snapshot: EngineSnapshot;
    onSnapshotChange: (next: EngineSnapshot) => void;
  }

  let { snapshot, onSnapshotChange }: Props = $props();

  let editingId = $state<string | null>(null);
  let screenName = $state("Audience 2 (Lobby / Overflow)");
  let screenKind = $state<"Audience" | "Stage">("Audience");
  let selectedMonitor = $state<string>("");
  let placementMode = $state<"windowed" | "fullscreen" | "region">("windowed");
  let winWidth = $state(1280);
  let winHeight = $state(720);
  let regionX = $state(0);
  let regionY = $state(0);
  let regionW = $state(960);
  let regionH = $state(1080);
  let selectedLookId = $state("");

  let splitKind = $state<"Audience" | "Stage">("Audience");
  let replaceExistingOnSplit = $state(true);
  let customCols = $state(2);
  let customRows = $state(1);

  let busy = $state(false);
  let notice = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  const displays = $derived(snapshot.displays ?? []);
  const outputs = $derived(snapshot.state.outputs ?? []);
  const looks = $derived<LookConfig[]>(Object.values(snapshot.state.looks ?? {}));

  function getRegion(out: OutputConfig): MonitorRegion | null {
    if (out.placement && typeof out.placement === "object" && "Region" in out.placement) {
      return out.placement.Region;
    }
    return null;
  }

  function placementBadge(out: OutputConfig): string {
    const reg = getRegion(out);
    if (reg) {
      return `REGION ${reg.width}×${reg.height} @ (${reg.x},${reg.y})`;
    }
    if (out.fullscreen || out.placement === "Fullscreen") {
      return "FULLSCREEN";
    }
    return `WINDOWED ${out.width}×${out.height}`;
  }

  function monitorLabel(idx: number | null): string {
    if (idx === null || idx === undefined) return "Unassigned (Windowed)";
    const d = displays.find((m) => m.index === idx);
    return d ? `Display ${d.index + 1} — ${d.name} (${d.width}×${d.height})` : `Display ${idx + 1}`;
  }

  function selectedDisplayObj(): DisplayInfo | null {
    if (selectedMonitor === "") return displays[0] ?? null;
    const idx = Number(selectedMonitor);
    return displays.find((d) => d.index === idx) ?? null;
  }

  function applyRegionFraction(fx: number, fy: number, fw: number, fh: number) {
    const mon = selectedDisplayObj();
    const mw = mon?.width ?? 1920;
    const mh = mon?.height ?? 1080;
    placementMode = "region";
    if (selectedMonitor === "" && mon) {
      selectedMonitor = String(mon.index);
    }
    regionX = Math.round(mw * fx);
    regionY = Math.round(mh * fy);
    regionW = Math.max(64, Math.round(mw * fw));
    regionH = Math.max(64, Math.round(mh * fh));
  }

  function startNewScreen(kind: "Audience" | "Stage", lowerThirdPreset = false) {
    editingId = null;
    screenKind = kind;
    const count = outputs.filter((o) => o.kind === kind).length + 1;
    if (lowerThirdPreset) {
      screenName = `Broadcast Lower-Third ${count}`;
      const ltLook = looks.find((l) => l.lower_third_mode);
      selectedLookId = ltLook?.id ?? "";
    } else {
      screenName = kind === "Audience" ? `Audience ${count}` : `Stage ${count}`;
      selectedLookId = "";
    }
    placementMode = "windowed";
    winWidth = kind === "Audience" ? 1280 : 960;
    winHeight = kind === "Audience" ? 720 : 540;
    notice = null;
    errorMsg = null;
  }

  function startEditing(out: OutputConfig) {
    editingId = out.id;
    screenName = out.name;
    screenKind = out.kind;
    selectedMonitor = out.monitor_index !== null && out.monitor_index !== undefined ? String(out.monitor_index) : "";
    selectedLookId = out.look_id;
    const reg = getRegion(out);
    if (reg) {
      placementMode = "region";
      regionX = reg.x;
      regionY = reg.y;
      regionW = reg.width;
      regionH = reg.height;
    } else if (out.fullscreen || out.placement === "Fullscreen") {
      placementMode = "fullscreen";
      winWidth = out.width;
      winHeight = out.height;
    } else {
      placementMode = "windowed";
      winWidth = out.width;
      winHeight = out.height;
    }
    notice = null;
    errorMsg = null;
  }

  async function saveScreen() {
    errorMsg = null;
    notice = null;
    if (!screenName.trim()) {
      errorMsg = "Please enter a screen name.";
      return;
    }
    busy = true;
    try {
      const monIdx = selectedMonitor === "" ? null : Number(selectedMonitor);
      const next = await invoke<EngineSnapshot>("upsert_output_screen", {
        screen: {
          id: editingId,
          name: screenName.trim(),
          kind: screenKind,
          monitor_index: monIdx,
          width: Number(winWidth),
          height: Number(winHeight),
          placement_mode: placementMode,
          region_x: Number(regionX),
          region_y: Number(regionY),
          region_width: Number(regionW),
          region_height: Number(regionH),
          look_id: selectedLookId || null,
        },
      });
      onSnapshotChange(next);
      notice = editingId
        ? `Updated screen "${screenName.trim()}" live.`
        : `Created ${screenKind} screen "${screenName.trim()}" live.`;
      editingId = null;
    } catch (err) {
      errorMsg = `Failed to save screen: ${String(err)}`;
    } finally {
      busy = false;
    }
  }

  async function removeScreen(out: OutputConfig) {
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const next = await invoke<EngineSnapshot>("remove_output_screen", {
        outputId: out.id,
      });
      if (editingId === out.id) editingId = null;
      onSnapshotChange(next);
      notice = `Removed screen "${out.name}".`;
    } catch (err) {
      errorMsg = `Failed to remove screen: ${String(err)}`;
    } finally {
      busy = false;
    }
  }

  async function assignFullscreenMonitor(display: DisplayInfo, kind: "Audience" | "Stage") {
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const next = await invoke<EngineSnapshot>("upsert_output_screen", {
        screen: {
          id: null,
          name: `Display ${display.index + 1} (${kind} Fullscreen)`,
          kind,
          monitor_index: display.index,
          width: display.width,
          height: display.height,
          placement_mode: "fullscreen",
          region_x: null,
          region_y: null,
          region_width: null,
          region_height: null,
          look_id: null,
        },
      });
      onSnapshotChange(next);
      notice = `Assigned Display ${display.index + 1} as Fullscreen ${kind} screen.`;
    } catch (err) {
      errorMsg = `Failed to assign monitor fullscreen: ${String(err)}`;
    } finally {
      busy = false;
    }
  }

  async function splitMonitor(display: DisplayInfo, cols: number, rows: number) {
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const next = await invoke<EngineSnapshot>("split_monitor_into_screens", {
        monitorIndex: display.index,
        cols,
        rows,
        defaultKind: splitKind,
        replaceExistingOnMonitor: replaceExistingOnSplit,
      });
      onSnapshotChange(next);
      notice = `Divided Display ${display.index + 1} (${display.width}×${display.height}) into ${cols}×${rows} independent ${splitKind} screens.`;
    } catch (err) {
      errorMsg = `Failed to split monitor: ${String(err)}`;
    } finally {
      busy = false;
    }
  }

  function regionStyle(display: DisplayInfo, out: OutputConfig): string {
    const reg = getRegion(out);
    if (reg && display.width > 0 && display.height > 0) {
      const left = Math.max(0, Math.min(100, (reg.x / display.width) * 100));
      const top = Math.max(0, Math.min(100, (reg.y / display.height) * 100));
      const w = Math.max(4, Math.min(100 - left, (reg.width / display.width) * 100));
      const h = Math.max(4, Math.min(100 - top, (reg.height / display.height) * 100));
      return `left:${left}%;top:${top}%;width:${w}%;height:${h}%;`;
    }
    return "left:0%;top:0%;width:100%;height:100%;";
  }
</script>

<section class="screens-panel" data-testid="screens-panel">
  <header class="panel-head">
    <div>
      <h2>Configure Screens & Physical Monitors</h2>
      <p class="sub">
        Create Audience or Stage outputs, assign full-screen physical monitors, or divide any monitor into independent borderless regions.
      </p>
    </div>
    <div class="quick-adds">
      <button class="btn primary" onclick={() => startNewScreen("Audience")}>+ New Audience</button>
      <button class="btn" onclick={() => startNewScreen("Audience", true)}>
        + New Broadcast Lower-Third
      </button>
      <button class="btn teal" onclick={() => startNewScreen("Stage")}>+ New Stage</button>
    </div>
  </header>

  {#if errorMsg}
    <div class="banner error" role="alert">{errorMsg}</div>
  {/if}
  {#if notice}
    <div class="banner ok" role="status">{notice}</div>
  {/if}

  <div class="section-card">
    <div class="card-head">
      <h3>Physical Displays ({displays.length} Detected)</h3>
      <div class="split-defaults">
        <label>
          Split As:
          <select aria-label="Split Default Kind" bind:value={splitKind}>
            <option value="Audience">Audience</option>
            <option value="Stage">Stage</option>
          </select>
        </label>
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={replaceExistingOnSplit} />
          Replace existing on monitor
        </label>
      </div>
    </div>

    {#if displays.length === 0}
      <p class="muted">No physical monitors reported yet (headless or initializing).</p>
    {:else}
      <div class="monitor-grid">
        {#each displays as display (display.index)}
          {@const assigned = outputs.filter((o) => o.monitor_index === display.index)}
          <div class="monitor-card">
            <div class="mon-meta">
              <strong>Display {display.index + 1}: {display.name}</strong>
              <span>
                {display.width}×{display.height} @ ({display.position_x},{display.position_y})
                {#if display.refresh_millihertz}
                  · {(display.refresh_millihertz / 1000).toFixed(0)} Hz
                {/if}
              </span>
            </div>

            <div class="mon-preview" aria-label={`Layout map for Display ${display.index + 1}`}>
              {#each assigned as out (out.id)}
                <button
                  class="region-tile"
                  class:stage={out.kind === "Stage"}
                  style={regionStyle(display, out)}
                  onclick={() => startEditing(out)}
                  title={`Click to edit ${out.name}`}
                >
                  <span>{out.name}</span>
                </button>
              {:else}
                <div class="mon-empty">Unassigned physical display</div>
              {/each}
            </div>

            <div class="divider-actions">
              <button
                class="btn sm"
                disabled={busy}
                onclick={() => void assignFullscreenMonitor(display, splitKind)}
              >
                Full Screen
              </button>
              <button
                class="btn sm"
                disabled={busy}
                onclick={() => void splitMonitor(display, 2, 1)}
              >
                Split 2×1 (L/R)
              </button>
              <button
                class="btn sm"
                disabled={busy}
                onclick={() => void splitMonitor(display, 1, 2)}
              >
                Split 1×2 (T/B)
              </button>
              <button
                class="btn sm"
                disabled={busy}
                onclick={() => void splitMonitor(display, 2, 2)}
              >
                Split 2×2 (4×)
              </button>
              <button
                class="btn sm"
                disabled={busy}
                onclick={() => void splitMonitor(display, 3, 1)}
              >
                Split 3×1
              </button>
              <div class="custom-split">
                <input
                  aria-label={`Custom Columns Display ${display.index + 1}`}
                  type="number"
                  min="1"
                  max="6"
                  bind:value={customCols}
                />
                <span>×</span>
                <input
                  aria-label={`Custom Rows Display ${display.index + 1}`}
                  type="number"
                  min="1"
                  max="6"
                  bind:value={customRows}
                />
                <button
                  class="btn sm"
                  disabled={busy}
                  onclick={() => void splitMonitor(display, customCols, customRows)}
                >
                  Split
                </button>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="two-col">
    <div class="section-card">
      <div class="card-head">
        <h3>{editingId ? "Edit Screen Configuration" : "Create New Screen"}</h3>
        {#if editingId}
          <button class="btn sm" onclick={() => (editingId = null)}>Cancel Edit</button>
        {/if}
      </div>

      <div class="form-grid">
        <label>
          Screen Name
          <input aria-label="Screen Name" bind:value={screenName} placeholder="e.g. Left LED Wall" />
        </label>

        <label>
          Screen Type
          <select aria-label="Screen Type" bind:value={screenKind}>
            <option value="Audience">Audience Display</option>
            <option value="Stage">Stage Confidence Display</option>
          </select>
        </label>

        <label>
          Physical Monitor
          <select aria-label="Physical Monitor" bind:value={selectedMonitor}>
            <option value="">Unassigned (Windowed)</option>
            {#each displays as d (d.index)}
              <option value={String(d.index)}>
                Display {d.index + 1} — {d.name} ({d.width}×{d.height})
              </option>
            {/each}
          </select>
        </label>

        <label>
          Placement Mode
          <select aria-label="Placement Mode" bind:value={placementMode}>
            <option value="windowed">Windowed (Resizable)</option>
            <option value="fullscreen">Full Screen on Monitor</option>
            <option value="region">Monitor Sub-Region (Borderless Area)</option>
          </select>
        </label>

        <label>
          Assigned Look
          <select aria-label="Assigned Look" bind:value={selectedLookId}>
            <option value="">Default for {screenKind}</option>
            {#each looks as look (look.id)}
              <option value={look.id}>{look.name}</option>
            {/each}
          </select>
        </label>
      </div>

      {#if placementMode === "windowed"}
        <div class="coords-row">
          <label>
            Width (px)
            <input aria-label="Window Width" type="number" min="320" bind:value={winWidth} />
          </label>
          <label>
            Height (px)
            <input aria-label="Window Height" type="number" min="180" bind:value={winHeight} />
          </label>
        </div>
      {:else if placementMode === "region"}
        <div class="region-presets">
          <span class="preset-label">Quick Area Presets:</span>
          <button class="btn xs" onclick={() => applyRegionFraction(0, 0, 0.5, 1)}>Left ½</button>
          <button class="btn xs" onclick={() => applyRegionFraction(0.5, 0, 0.5, 1)}>Right ½</button>
          <button class="btn xs" onclick={() => applyRegionFraction(0, 0, 1, 0.5)}>Top ½</button>
          <button class="btn xs" onclick={() => applyRegionFraction(0, 0.5, 1, 0.5)}>Bottom ½</button>
          <button class="btn xs" onclick={() => applyRegionFraction(0, 0, 0.5, 0.5)}>TL ¼</button>
          <button class="btn xs" onclick={() => applyRegionFraction(0.5, 0, 0.5, 0.5)}>TR ¼</button>
          <button class="btn xs" onclick={() => applyRegionFraction(0, 0.5, 0.5, 0.5)}>BL ¼</button>
          <button class="btn xs" onclick={() => applyRegionFraction(0.5, 0.5, 0.5, 0.5)}>BR ¼</button>
        </div>
        <div class="coords-row">
          <label>
            Region X (px)
            <input aria-label="Region X" type="number" bind:value={regionX} />
          </label>
          <label>
            Region Y (px)
            <input aria-label="Region Y" type="number" bind:value={regionY} />
          </label>
          <label>
            Region Width (px)
            <input aria-label="Region Width" type="number" min="64" bind:value={regionW} />
          </label>
          <label>
            Region Height (px)
            <input aria-label="Region Height" type="number" min="64" bind:value={regionH} />
          </label>
        </div>
      {/if}

      <div class="form-footer">
        <button class="btn primary" disabled={busy} onclick={() => void saveScreen()}>
          {editingId ? "Apply Changes Live" : `Create ${screenKind} Screen`}
        </button>
      </div>
    </div>

    <div class="section-card">
      <div class="card-head">
        <h3>Configured Outputs ({outputs.length})</h3>
      </div>
      {#if outputs.length === 0}
        <p class="muted">No active outputs configured. Create one or split a monitor above.</p>
      {:else}
        <ul class="output-list">
          {#each outputs as out (out.id)}
            <li class="output-row" class:editing={editingId === out.id}>
              <div class="out-main">
                <div class="out-title">
                  <span class="kind-pill" class:stage={out.kind === "Stage"}>
                    {out.kind.toUpperCase()}
                  </span>
                  <strong>{out.name}</strong>
                </div>
                <div class="out-meta">
                  <span>{monitorLabel(out.monitor_index)}</span>
                  <span>·</span>
                  <code>{placementBadge(out)}</code>
                </div>
              </div>
              <div class="out-actions">
                <button class="btn sm" onclick={() => startEditing(out)}>Edit</button>
                <button class="btn sm danger" disabled={busy} onclick={() => void removeScreen(out)}>
                  Remove
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
</section>

<style>
  .screens-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    overflow-y: auto;
    height: 100%;
  }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .panel-head h2 {
    margin: 0;
    font-size: 16px;
  }

  .sub {
    margin: 3px 0 0;
    font-size: 12px;
    color: var(--text-1);
  }

  .quick-adds,
  .split-defaults,
  .out-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--text-1);
  }

  .banner {
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 12px;
  }

  .banner.error {
    background: rgba(239, 68, 68, 0.16);
    border: 1px solid rgba(239, 68, 68, 0.5);
    color: #fecaca;
  }

  .banner.ok {
    background: rgba(16, 185, 129, 0.16);
    border: 1px solid rgba(16, 185, 129, 0.45);
    color: #a7f3d0;
  }

  .section-card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .card-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .card-head h3 {
    margin: 0;
    font-size: 13px;
  }

  .monitor-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(290px, 1fr));
    gap: 10px;
  }

  .monitor-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .mon-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
  }

  .mon-meta span {
    color: var(--text-1);
    font-size: 11px;
  }

  .mon-preview {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    background: #0a0e17;
    border: 1px solid #2b354b;
    border-radius: 6px;
    overflow: hidden;
  }

  .mon-empty {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--text-1);
    font-size: 11px;
  }

  .region-tile {
    position: absolute;
    background: rgba(37, 99, 235, 0.42);
    border: 1px solid #60a5fa;
    color: #fff;
    font-size: 10px;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 4px;
    text-align: center;
    cursor: pointer;
    overflow: hidden;
  }

  .region-tile.stage {
    background: rgba(15, 118, 110, 0.48);
    border-color: #2dd4bf;
  }

  .divider-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    align-items: center;
  }

  .custom-split {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }

  .custom-split input {
    width: 44px;
    padding: 3px 5px;
  }

  .two-col {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .form-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
    gap: 8px;
  }

  .coords-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(105px, 1fr));
    gap: 8px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--text-1);
  }

  input,
  select {
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font-size: 12px;
  }

  .region-presets {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    align-items: center;
  }

  .preset-label {
    font-size: 11px;
    color: var(--text-1);
  }

  .form-footer {
    display: flex;
    justify-content: flex-end;
    margin-top: 4px;
  }

  .output-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .output-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  .output-row.editing {
    border-color: #3b82f6;
  }

  .out-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }

  .out-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    font-size: 11px;
    color: var(--text-1);
    margin-top: 3px;
  }

  .kind-pill {
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 700;
    background: #1d4ed8;
    color: #fff;
  }

  .kind-pill.stage {
    background: #0f766e;
  }

  .btn {
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 10px;
    font-size: 12px;
    cursor: pointer;
  }

  .btn.primary {
    background: #1d4ed8;
    border-color: #2563eb;
    color: #fff;
  }

  .btn.teal {
    background: #0f766e;
    border-color: #14b8a6;
    color: #fff;
  }

  .btn.danger {
    border-color: rgba(239, 68, 68, 0.5);
    color: #fca5a5;
  }

  .btn.sm {
    padding: 4px 8px;
    font-size: 11px;
  }

  .btn.xs {
    padding: 3px 6px;
    font-size: 11px;
  }

  .btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .muted {
    color: var(--text-1);
    font-size: 12px;
    margin: 0;
  }
</style>
