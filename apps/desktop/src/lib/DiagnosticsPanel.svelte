<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { CrashReport, DiagnosticsStatus, ExportedBundle } from "./types";

  let status = $state<DiagnosticsStatus | null>(null);
  let crashes = $state<CrashReport[]>([]);
  let logLines = $state<string[]>([]);
  let logSource = $state<"all" | "engine" | "desktop">("all");
  let selectedLevel = $state("info");
  let exportedBundle = $state<ExportedBundle | null>(null);
  let actionNotice = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  async function refreshAll() {
    errorMsg = null;
    try {
      const [st, reports, logs] = await Promise.all([
        invoke<DiagnosticsStatus>("get_diagnostics_status"),
        invoke<CrashReport[]>("list_crash_reports"),
        invoke<string[]>("read_diagnostic_logs", { source: logSource, maxLines: 150 }),
      ]);
      status = st;
      selectedLevel = st.active_log_level;
      crashes = reports;
      logLines = logs;
    } catch (err) {
      errorMsg = `Failed to load diagnostics: ${String(err)}`;
    }
  }

  async function updateLogLevel(level: string) {
    errorMsg = null;
    actionNotice = null;
    try {
      const applied = await invoke<string>("set_debug_logging", { level });
      selectedLevel = applied;
      if (status) {
        status = { ...status, active_log_level: applied };
      }
      actionNotice = `Log level updated live to "${applied}" (Desktop + Engine).`;
    } catch (err) {
      errorMsg = `Failed to change log level: ${String(err)}`;
    }
  }

  async function refreshLogs() {
    errorMsg = null;
    try {
      logLines = await invoke<string[]>("read_diagnostic_logs", {
        source: logSource,
        maxLines: 150,
      });
    } catch (err) {
      errorMsg = `Failed to read logs: ${String(err)}`;
    }
  }

  async function exportBundle() {
    errorMsg = null;
    actionNotice = null;
    try {
      const bundle = await invoke<ExportedBundle>("export_diagnostics_bundle");
      exportedBundle = bundle;
      actionNotice = `Saved redacted diagnostic bundle (${bundle.crash_count} crash reports) to ${bundle.file_path}`;
    } catch (err) {
      errorMsg = `Bundle export failed: ${String(err)}`;
    }
  }

  async function clearCrashes() {
    errorMsg = null;
    actionNotice = null;
    try {
      const removed = await invoke<number>("clear_crash_reports");
      crashes = [];
      if (status) {
        status = { ...status, crash_count: 0 };
      }
      actionNotice = `Cleared ${removed} crash report(s).`;
    } catch (err) {
      errorMsg = `Failed to clear crash reports: ${String(err)}`;
    }
  }

  onMount(() => {
    void refreshAll();
  });
</script>

<section class="diag-panel" data-testid="diagnostics-panel">
  <header class="diag-header">
    <div>
      <h2>Debug Logging & Crash Reports</h2>
      <p class="subtitle">
        Local-first rolling logs & offline crash capture (zero automatic telemetry).
      </p>
    </div>
    <div class="header-actions">
      <label class="level-picker">
        Log Level:
        <select
          aria-label="Log Verbosity Level"
          value={selectedLevel}
          onchange={(e) => void updateLogLevel(e.currentTarget.value)}
        >
          <option value="error">Error</option>
          <option value="warn">Warn</option>
          <option value="info">Info (Default)</option>
          <option value="debug">Debug (Verbose)</option>
          <option value="trace">Trace (Full)</option>
        </select>
      </label>
      <button class="btn" onclick={() => void refreshAll()}>Refresh</button>
      <button class="btn primary" onclick={() => void exportBundle()}>
        Export Diagnostic Bundle
      </button>
    </div>
  </header>

  {#if errorMsg}
    <div class="alert error" role="alert">{errorMsg}</div>
  {/if}
  {#if actionNotice}
    <div class="alert notice" role="status">{actionNotice}</div>
  {/if}

  {#if status}
    <div class="path-grid">
      <div class="path-card">
        <span class="label">Log Directory</span>
        <code>{status.log_dir}</code>
      </div>
      <div class="path-card">
        <span class="label">Crash Directory</span>
        <code>{status.crash_dir}</code>
      </div>
      <div class="path-card">
        <span class="label">Active Level</span>
        <strong>{status.active_log_level.toUpperCase()}</strong>
      </div>
      <div class="path-card">
        <span class="label">Recorded Crashes</span>
        <strong class:warn={crashes.length > 0}>{crashes.length}</strong>
      </div>
    </div>
  {/if}

  <div class="diag-columns">
    <div class="card">
      <div class="card-head">
        <h3>Crash & GPU Error Reports ({crashes.length})</h3>
        <button
          class="btn small"
          onclick={() => void clearCrashes()}
          disabled={crashes.length === 0}
        >
          Clear Reports
        </button>
      </div>
      {#if crashes.length === 0}
        <p class="empty">No crash reports recorded.</p>
      {:else}
        <ul class="crash-list">
          {#each crashes as report (report.id)}
            <li class="crash-item">
              <div class="crash-top">
                <span class="badge">{report.component}</span>
                <span class="kind">{report.kind}</span>
                <span class="ts">{report.timestamp_utc}</span>
              </div>
              <p class="crash-msg">{report.message}</p>
              {#if report.location}
                <div class="crash-loc">{report.location}</div>
              {/if}
              <details>
                <summary>Backtrace & Context</summary>
                <pre class="code-box">{report.backtrace}</pre>
              </details>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="card">
      <div class="card-head">
        <h3>Rolling Live Logs</h3>
        <div class="log-controls">
          <select
            aria-label="Log Source"
            bind:value={logSource}
            onchange={() => void refreshLogs()}
          >
            <option value="all">All (Engine + Desktop)</option>
            <option value="engine">Engine (`engine.log`)</option>
            <option value="desktop">Desktop (`desktop.log`)</option>
          </select>
          <button class="btn small" onclick={() => void refreshLogs()}>Reload Tail</button>
        </div>
      </div>
      {#if logLines.length === 0}
        <p class="empty">No log entries recorded yet.</p>
      {:else}
        <pre class="log-viewer" data-testid="log-viewer">{logLines.join("\n")}</pre>
      {/if}
    </div>
  </div>

  {#if exportedBundle}
    <div class="card bundle-preview">
      <div class="card-head">
        <h3>Redacted Bundle Preview</h3>
        <code>{exportedBundle.file_path}</code>
      </div>
      <pre class="code-box">{exportedBundle.markdown}</pre>
    </div>
  {/if}
</section>

<style>
  .diag-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    overflow-y: auto;
    height: 100%;
  }

  .diag-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .diag-header h2 {
    margin: 0;
    font-size: 16px;
  }

  .subtitle {
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--text-1);
  }

  .header-actions,
  .log-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .level-picker {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-1);
  }

  select {
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
    font-size: 12px;
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

  .btn.small {
    padding: 4px 8px;
    font-size: 11px;
  }

  .btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .alert {
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 12px;
  }

  .alert.error {
    background: rgba(239, 68, 68, 0.16);
    border: 1px solid rgba(239, 68, 68, 0.5);
    color: #fecaca;
  }

  .alert.notice {
    background: rgba(16, 185, 129, 0.15);
    border: 1px solid rgba(16, 185, 129, 0.45);
    color: #a7f3d0;
  }

  .path-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 8px;
  }

  .path-card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
  }

  .path-card .label {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-1);
    letter-spacing: 0.05em;
  }

  .path-card code {
    font-size: 11px;
    word-break: break-all;
    color: var(--text-0);
  }

  .path-card strong.warn {
    color: #f87171;
  }

  .diag-columns {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    min-height: 280px;
  }

  .card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .card-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .card-head h3 {
    margin: 0;
    font-size: 13px;
  }

  .crash-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 320px;
    overflow-y: auto;
  }

  .crash-item {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
    font-size: 12px;
  }

  .crash-top {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    margin-bottom: 4px;
  }

  .badge {
    background: #b91c1c;
    color: #fff;
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 700;
  }

  .kind {
    font-family: monospace;
    font-size: 11px;
    color: #fca5a5;
  }

  .ts {
    margin-left: auto;
    font-size: 10px;
    color: var(--text-1);
  }

  .crash-msg {
    margin: 4px 0;
    font-weight: 600;
  }

  .crash-loc {
    font-family: monospace;
    font-size: 11px;
    color: var(--text-1);
    margin-bottom: 4px;
  }

  .log-viewer,
  .code-box {
    margin: 0;
    padding: 8px;
    background: #0a0d14;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
    font-size: 11px;
    line-height: 1.45;
    max-height: 320px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .empty {
    color: var(--text-1);
    font-size: 12px;
    margin: 8px 0;
  }
</style>
