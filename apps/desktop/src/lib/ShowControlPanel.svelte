<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

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

  let importKind = $state<"chordpro" | "songselect_usr" | "plain_text" | "bible_osis">("chordpro");
  let importPayload = $state("{title: Amazing Grace}\n{ccli: 22025}\n[G]Amazing [C]grace! How sweet the sound");
  let importResult = $state<ImportSummary | null>(null);
  let ccliCsv = $state<string | null>(null);
  let panelError = $state<string | null>(null);

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
</script>

<div class="m3-panel" data-testid="ecosystem-panel">
  <h2>Show Control, Importers, CCLI & Ecosystem</h2>

  {#if panelError}
    <div class="alert err" role="alert">{panelError}</div>
  {/if}

  <div class="section">
    <h3>Concurrent Timers (Audience & Stage Routable)</h3>
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
            <span class="clock" class:overrun={t.overrun}>{t.formatted}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="section">
    <h3>Clean-Room Importer (ChordPro, SongSelect .usr, Plain Text, Bible OSIS)</h3>
    <div class="row">
      <select aria-label="Import Format" bind:value={importKind}>
        <option value="chordpro">ChordPro (.cho / .crd)</option>
        <option value="songselect_usr">CCLI SongSelect (.usr)</option>
        <option value="plain_text">Plain Text Stanzas</option>
        <option value="bible_osis">Bible XML (OSIS)</option>
      </select>
      <button class="btn primary" onclick={runImport}>Import to Local Library</button>
    </div>
    <textarea aria-label="Import Content" rows="4" bind:value={importPayload}></textarea>
    {#if importResult}
      <div class="alert ok" role="status">
        Imported "{importResult.title}" ({importResult.slide_count} item(s), {importResult.source_format})
      </div>
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

<style>
  .m3-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    overflow-y: auto;
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

  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
  }
</style>
