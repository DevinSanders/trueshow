<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import CcliDisplaySettingsPanel from "./CcliDisplaySettingsPanel.svelte";
  import PcoPdfImporterPanel from "./PcoPdfImporterPanel.svelte";
  import type { MacroRecord, SceneState, ScheduledEventRecord } from "./types";

  interface Props {
    onSceneChange?: (next: SceneState) => void;
  }

  let { onSceneChange }: Props = $props();

  interface TimerStatus {
    id: string;
    name: string;
    formatted: string;
    running: boolean;
    overrun: boolean;
  }

  interface ImportSummary {
    title: string;
    slide_count: number;
    ccli_number: string | null;
    source_format: string;
  }

  let timerName = $state("Sermon Countdown");
  let timerSeconds = $state(1800);
  let timers = $state<TimerStatus[]>([]);

  let macros = $state<MacroRecord[]>([]);
  let macroName = $state("Worship Start");
  let macroStepsInput = $state("blackout:off\noverlay:show:overlay-church-bug");

  let scheduledEvents = $state<ScheduledEventRecord[]>([]);
  let schedName = $state("Sunday 10am Kickoff");
  let schedTime = $state("10:00");
  let schedMacroId = $state("");

  let controlProtocol = $state<"osc" | "companion" | "macro">("osc");
  let controlCommand = $state("/openpresenter/blackout 0");
  let controlResult = $state<string | null>(null);

  let importKind = $state<
    | "chordpro"
    | "songselect_usr"
    | "plain_text"
    | "bible_osis"
    | "freeshow_json"
    | "opensong_xml"
    | "propresenter_xml"
  >("chordpro");
  let importPayload = $state("{title: Amazing Grace}\n{ccli: 22025}\n[G]Amazing [C]grace! How sweet the sound");
  let importFilePath = $state("");
  let importResult = $state<ImportSummary | null>(null);
  let backupPath = $state("");
  let backupStatus = $state<string | null>(null);
  let ccliCsv = $state<string | null>(null);
  let panelError = $state<string | null>(null);

  async function refreshAutomation() {
    try {
      const [tList, mList, sList] = await Promise.all([
        invoke<TimerStatus[]>("list_timers").catch(() => []),
        invoke<MacroRecord[]>("list_macros").catch(() => []),
        invoke<ScheduledEventRecord[]>("list_scheduled_events").catch(() => []),
      ]);
      timers = Array.isArray(tList) ? tList : [];
      macros = Array.isArray(mList) ? mList : [];
      scheduledEvents = Array.isArray(sList) ? sList : [];
      if (macros.length > 0 && !schedMacroId) {
        schedMacroId = macros[0].id;
      }
    } catch {
      // ignore offline mock failures on mount
    }
  }

  async function createTimer() {
    panelError = null;
    if (timerName.trim().length === 0 || timerSeconds <= 0) {
      panelError = "Timer name and positive duration are required";
      return;
    }
    try {
      timers = await invoke<TimerStatus[]>("create_countdown_timer", {
        name: timerName.trim(),
        initialSecs: timerSeconds,
      });
    } catch (err) {
      panelError = String(err);
    }
  }

  async function controlTimer(timerId: string, action: "start" | "pause" | "reset" | "add_minute") {
    panelError = null;
    try {
      timers = await invoke<TimerStatus[]>("control_timer", { timerId, action });
    } catch (err) {
      panelError = String(err);
    }
  }

  async function saveMacro() {
    panelError = null;
    const steps = macroStepsInput
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean);
    if (!macroName.trim() || steps.length === 0) {
      panelError = "Macro name and at least one step are required";
      return;
    }
    try {
      macros = await invoke<MacroRecord[]>("save_macro", {
        id: null,
        name: macroName.trim(),
        steps,
      });
    } catch (err) {
      panelError = String(err);
    }
  }

  async function triggerMacro(macroId: string) {
    panelError = null;
    try {
      const maybeScene = await invoke<SceneState | null>("run_macro", { macroId });
      if (maybeScene) onSceneChange?.(maybeScene);
      controlResult = "Executed Macro live.";
    } catch (err) {
      panelError = String(err);
    }
  }

  async function removeMacro(macroId: string) {
    panelError = null;
    try {
      macros = await invoke<MacroRecord[]>("delete_macro", { macroId });
    } catch (err) {
      panelError = String(err);
    }
  }

  async function addScheduledEvent() {
    panelError = null;
    if (!schedName.trim() || schedTime.trim().length !== 5) {
      panelError = "Provide event name and HH:MM trigger time";
      return;
    }
    try {
      scheduledEvents = await invoke<ScheduledEventRecord[]>("save_scheduled_event", {
        id: null,
        name: schedName.trim(),
        triggerHhmm: schedTime.trim(),
        macroId: schedMacroId || (macros[0]?.id ?? ""),
        enabled: true,
      });
    } catch (err) {
      panelError = String(err);
    }
  }

  async function removeScheduledEvent(eventId: string) {
    panelError = null;
    try {
      scheduledEvents = await invoke<ScheduledEventRecord[]>("delete_scheduled_event", { eventId });
    } catch (err) {
      panelError = String(err);
    }
  }

  async function sendControlCommand() {
    panelError = null;
    controlResult = null;
    try {
      controlResult = await invoke<string>("dispatch_control_command", {
        protocol: controlProtocol,
        command: controlCommand,
      });
    } catch (err) {
      panelError = String(err);
    }
  }

  async function runImport() {
    panelError = null;
    importResult = null;
    if (importPayload.trim().length === 0) {
      panelError = "Paste document content before importing";
      return;
    }
    try {
      importResult = await invoke<ImportSummary>("import_worship_document", {
        format: importKind,
        content: importPayload,
      });
    } catch (err) {
      panelError = String(err);
    }
  }

  async function exportCcliReport() {
    panelError = null;
    try {
      ccliCsv = await invoke<string>("export_ccli_csv_report");
    } catch (err) {
      panelError = String(err);
    }
  }

  async function importFromFile() {
    panelError = null;
    importResult = null;
    if (!importFilePath.trim()) {
      panelError = "Enter a local file path (.pptx, .show, .pro6, .xml, .cho, .usr)";
      return;
    }
    try {
      importResult = await invoke<ImportSummary>("import_worship_file", {
        format: "auto",
        filePath: importFilePath.trim(),
      });
    } catch (err) {
      panelError = String(err);
    }
  }

  async function exportBackup() {
    panelError = null;
    backupStatus = null;
    try {
      const res = await invoke<{ bundle_path: string; item_count: number }>("export_library_backup");
      backupPath = res.bundle_path;
      backupStatus = `Exported ${res.item_count} library items to ${res.bundle_path}`;
    } catch (err) {
      panelError = String(err);
    }
  }

  async function restoreBackup() {
    panelError = null;
    backupStatus = null;
    if (!backupPath.trim()) {
      panelError = "Provide a .opbundle backup file path to restore";
      return;
    }
    try {
      const res = await invoke<{ bundle_path: string; item_count: number }>("import_library_backup", {
        filePath: backupPath.trim(),
      });
      backupStatus = `Restored ${res.item_count} library items from ${res.bundle_path}`;
      await refreshAutomation();
    } catch (err) {
      panelError = String(err);
    }
  }

  onMount(() => {
    void refreshAutomation();
    const handle = setInterval(() => {
      if (timers.some((t) => t.running)) {
        void invoke<TimerStatus[]>("tick_timers", { elapsedMs: 1000 })
          .then((list) => {
            if (Array.isArray(list)) timers = list;
          })
          .catch(() => {});
      }
    }, 1000);
    return () => clearInterval(handle);
  });
</script>

<div class="m3-panel" data-testid="ecosystem-panel">
  <h2>Show Control, Live Timers, Macros, Calendar & Importers</h2>

  {#if panelError}
    <div class="alert err" role="alert">{panelError}</div>
  {/if}
  {#if controlResult}
    <div class="alert ok" role="status">{controlResult}</div>
  {/if}

  <div class="section">
    <h3>Concurrent Timers (Live Ticking → Audience & Stage)</h3>
    <div class="row">
      <input aria-label="Timer Name" bind:value={timerName} />
      <input aria-label="Timer Seconds" type="number" min="1" bind:value={timerSeconds} />
      <button class="btn primary" onclick={createTimer}>Add Countdown</button>
    </div>
    {#if timers.length > 0}
      <ul class="timer-list">
        {#each timers as t (t.id)}
          <li>
            <strong>{t.name}</strong>
            <div class="row">
              <span class="clock" class:overrun={t.overrun}>{t.formatted}</span>
              <button class="btn xs" onclick={() => void controlTimer(t.id, t.running ? "pause" : "start")}>
                {t.running ? "Pause" : "Start"}
              </button>
              <button class="btn xs" onclick={() => void controlTimer(t.id, "add_minute")}>+1m</button>
              <button class="btn xs" onclick={() => void controlTimer(t.id, "reset")}>Reset</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="two-col">
    <div class="section">
      <h3>Macros & One-Click Automation</h3>
      <div class="macro-chips">
        {#each macros as m (m.id)}
          <div class="macro-pill">
            <button class="btn xs primary" onclick={() => void triggerMacro(m.id)}>▶ {m.name}</button>
            <button class="btn xs danger" aria-label={`Delete macro ${m.name}`} onclick={() => void removeMacro(m.id)}>×</button>
          </div>
        {/each}
      </div>
      <input aria-label="Macro Name" bind:value={macroName} placeholder="Macro name" />
      <textarea aria-label="Macro Steps" rows="2" bind:value={macroStepsInput}></textarea>
      <button class="btn sm" onclick={() => void saveMacro()}>Save Macro</button>
    </div>

    <div class="section">
      <h3>Calendar Schedule & OSC / Companion Control</h3>
      <div class="row">
        <input aria-label="Schedule Event Name" bind:value={schedName} />
        <input aria-label="Schedule Time HH:MM" bind:value={schedTime} style="max-width:78px;" />
        <button class="btn sm primary" onclick={() => void addScheduledEvent()}>Schedule</button>
      </div>
      {#if scheduledEvents.length > 0}
        <ul class="timer-list">
          {#each scheduledEvents as ev (ev.id)}
            <li>
              <span><strong>{ev.trigger_hhmm}</strong> · {ev.name}</span>
              <button class="btn xs danger" onclick={() => void removeScheduledEvent(ev.id)}>×</button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="row">
        <select aria-label="Control Protocol" bind:value={controlProtocol}>
          <option value="osc">OSC</option>
          <option value="companion">Bitfocus Companion</option>
          <option value="macro">Macro Step</option>
        </select>
        <input aria-label="Control Command" bind:value={controlCommand} />
        <button class="btn sm" onclick={() => void sendControlCommand()}>Send</button>
      </div>
    </div>
  </div>

  <CcliDisplaySettingsPanel {onSceneChange} />

  <PcoPdfImporterPanel {onSceneChange} />

  <div class="section">
    <h3>Clean-Room Importer (ChordPro, SongSelect .usr, Plain Text, Bible OSIS, FreeShow .show, OpenSong, ProPresenter)</h3>
    <div class="row">
      <select aria-label="Import Format" bind:value={importKind}>
        <option value="chordpro">ChordPro (.cho / .crd)</option>
        <option value="songselect_usr">CCLI SongSelect (.usr)</option>
        <option value="plain_text">Plain Text Stanzas</option>
        <option value="bible_osis">Bible XML (OSIS)</option>
        <option value="freeshow_json">FreeShow (.show JSON)</option>
        <option value="opensong_xml">OpenSong XML</option>
        <option value="propresenter_xml">ProPresenter (.pro6 XML)</option>
      </select>
      <button class="btn primary" onclick={runImport}>Import to Local Library</button>
    </div>
    <textarea aria-label="Import Content" rows="4" bind:value={importPayload}></textarea>
    <div class="row">
      <input
        aria-label="Import File Path"
        placeholder="Local file path (.pptx, .show, .pro6, .xml, .cho, .usr)"
        bind:value={importFilePath}
      />
      <button class="btn" onclick={() => void importFromFile()}>Import File</button>
    </div>
    {#if importResult}
      <div class="alert ok" role="status">
        Imported "{importResult.title}" ({importResult.slide_count} item(s), {importResult.source_format})
      </div>
    {/if}
  </div>

  <div class="two-col">
    <div class="section">
      <h3>Portable Full Library Backup (.opbundle)</h3>
      <div class="row">
        <button class="btn primary" onclick={() => void exportBackup()}>Export .opbundle Backup</button>
      </div>
      <div class="row">
        <input
          aria-label="Backup Bundle Path"
          placeholder="Path to .opbundle file to restore"
          bind:value={backupPath}
        />
        <button class="btn" onclick={() => void restoreBackup()}>Restore</button>
      </div>
      {#if backupStatus}
        <div class="alert ok" role="status">{backupStatus}</div>
      {/if}
    </div>

    <div class="section">
      <h3>CCLI Reporting (Local Usage Log -> CSV)</h3>
      <button class="btn" onclick={exportCcliReport}>Export CCLI CSV Report</button>
      {#if ccliCsv}
        <pre class="csv-box" data-testid="ccli-csv">{ccliCsv}</pre>
      {/if}
    </div>
  </div>
</div>

<style>
  .m3-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    overflow-y: auto;
  }

  .two-col {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  h2,
  h3 {
    margin: 0 0 6px 0;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-1);
  }

  .section {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .macro-chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .macro-pill {
    display: inline-flex;
    gap: 3px;
  }

  input,
  select,
  textarea {
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font: inherit;
    font-size: 12px;
  }

  input,
  select {
    flex: 1;
  }

  .timer-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .timer-list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 8px;
    background: var(--bg-2);
    border-radius: 6px;
    font-size: 12px;
  }

  .clock {
    font-variant-numeric: tabular-nums;
    color: var(--ok);
    font-weight: 700;
  }

  .clock.overrun {
    color: var(--danger);
  }

  .csv-box {
    margin: 0;
    padding: 8px;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    font-size: 11px;
    max-height: 120px;
    overflow: auto;
  }

  .alert {
    padding: 8px 10px;
    border-radius: 6px;
    font-size: 12px;
  }

  .alert.err {
    background: rgba(239, 68, 68, 0.18);
    color: #fecaca;
  }

  .alert.ok {
    background: rgba(34, 197, 94, 0.16);
    color: #bbf7d0;
  }

  .btn {
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 12px;
    font-weight: 600;
    align-self: flex-start;
  }

  .btn.sm {
    padding: 4px 9px;
    font-size: 11px;
  }

  .btn.xs {
    padding: 2px 7px;
    font-size: 10px;
  }

  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
  }

  .btn.danger {
    background: rgba(239, 68, 68, 0.18);
    color: #fecaca;
  }
</style>
