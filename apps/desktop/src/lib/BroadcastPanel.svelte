<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  interface BroadcastCapabilities {
    ndi_status: string;
    omt_ready: boolean;
    decklink_status: string;
    virtual_output_backend: string;
    web_stage_url: string;
  }

  let caps = $state<BroadcastCapabilities | null>(null);
  let loadError = $state<string | null>(null);
  let ndiSourceName = $state("OpenPresenter — Main Output");
  let alphaMode = $state<"StraightBgra" | "PremultipliedBgra" | "KeyFillSplit">("StraightBgra");
  let decklinkMode = $state<"ExternalDualSdiKeyFill" | "InternalKeyer">("ExternalDualSdiKeyFill");
  let saveMessage = $state<string | null>(null);

  async function refreshStatus() {
    loadError = null;
    try {
      caps = await invoke<BroadcastCapabilities>("get_broadcast_capabilities");
    } catch (err) {
      loadError = String(err);
    }
  }

  function applyConfig() {
    saveMessage = null;
    if (ndiSourceName.trim().length === 0) {
      loadError = "Broadcast source name cannot be empty";
      return;
    }
    loadError = null;
    saveMessage = `Configured "${ndiSourceName.trim()}" (${alphaMode} / ${decklinkMode})`;
  }

  onMount(() => {
    void refreshStatus();
  });
</script>

<div class="broadcast-panel" data-testid="broadcast-panel">
  <div class="header">
    <h2>Broadcast Video I/O & Virtual Outputs</h2>
    <button class="btn" onclick={refreshStatus}>Probe Runtimes</button>
  </div>

  {#if loadError}
    <div class="alert err" role="alert">{loadError}</div>
  {/if}
  {#if saveMessage}
    <div class="alert ok" role="status">{saveMessage}</div>
  {/if}

  {#if caps}
    <div class="status-grid">
      <div class="card">
        <h3>NDI 6.x (Dynamic Runtime)</h3>
        <p>{caps.ndi_status}</p>
      </div>
      <div class="card">
        <h3>Open Media Transport (OMT)</h3>
        <p>{caps.omt_ready ? "Ready (MIT LAN + Alpha + mDNS)" : "Disabled"}</p>
      </div>
      <div class="card">
        <h3>Blackmagic DeckLink SDI</h3>
        <p>{caps.decklink_status}</p>
      </div>
      <div class="card">
        <h3>Virtual Output (OBS / vMix)</h3>
        <p>{caps.virtual_output_backend}</p>
      </div>
      <div class="card wide">
        <h3>LAN Browser Stage Display</h3>
        <p><code>{caps.web_stage_url}</code></p>
      </div>
    </div>
  {/if}

  <div class="form">
    <label>
      <span>NDI / OMT Source Name</span>
      <input aria-label="Broadcast Source Name" bind:value={ndiSourceName} />
    </label>
    <label>
      <span>Alpha Channel Mode</span>
      <select aria-label="Alpha Mode" bind:value={alphaMode}>
        <option value="StraightBgra">Straight BGRA (NDI / OMT Lower Thirds)</option>
        <option value="PremultipliedBgra">Premultiplied BGRA</option>
        <option value="KeyFillSplit">Dual-SDI Key & Fill Split (DeckLink)</option>
      </select>
    </label>
    <label>
      <span>DeckLink Keying Mode</span>
      <select aria-label="DeckLink Key Mode" bind:value={decklinkMode}>
        <option value="ExternalDualSdiKeyFill">External Dual-SDI Key & Fill (ATEM / Ross)</option>
        <option value="InternalKeyer">Internal Hardware Keyer</option>
      </select>
    </label>
    <button class="btn primary" onclick={applyConfig}>Apply Broadcast Profile</button>
  </div>
</div>

<style>
  .broadcast-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    overflow-y: auto;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .header h2 {
    margin: 0;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-1);
  }

  .status-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  .card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
  }

  .card.wide {
    grid-column: span 2;
  }

  .card h3 {
    margin: 0 0 4px 0;
    font-size: 12px;
    color: var(--text-1);
  }

  .card p {
    margin: 0;
    font-size: 12px;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
  }

  .form label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--text-1);
  }

  .form input,
  .form select {
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font-size: 12px;
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
