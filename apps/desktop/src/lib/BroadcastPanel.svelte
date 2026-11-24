<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  interface BroadcastCapabilities {
    ndi_status: string;
    omt_ready: boolean;
    decklink_status: string;
    virtual_output_backend: string;
    web_stage_url: string;
    web_musician_url?: string;
    web_remote_url?: string;
    remote_pin?: string;
    active_source_name?: string;
    alpha_mode?: "StraightBgra" | "PremultipliedBgra" | "KeyFillSplit";
    decklink_mode?: "ExternalDualSdiKeyFill" | "InternalKeyer";
    streaming_active?: boolean;
  }

  let caps = $state<BroadcastCapabilities | null>(null);
  let loadError = $state<string | null>(null);
  let ndiSourceName = $state("OpenPresenter — Main Output");
  let alphaMode = $state<"StraightBgra" | "PremultipliedBgra" | "KeyFillSplit">("StraightBgra");
  let decklinkMode = $state<"ExternalDualSdiKeyFill" | "InternalKeyer">("ExternalDualSdiKeyFill");
  let streamingActive = $state(false);
  let saveMessage = $state<string | null>(null);

  async function refreshStatus() {
    loadError = null;
    try {
      const next = await invoke<BroadcastCapabilities>("get_broadcast_capabilities");
      caps = next;
      if (next.active_source_name) ndiSourceName = next.active_source_name;
      if (next.alpha_mode) alphaMode = next.alpha_mode;
      if (next.decklink_mode) decklinkMode = next.decklink_mode;
      if (typeof next.streaming_active === "boolean") streamingActive = next.streaming_active;
    } catch (err) {
      loadError = String(err);
    }
  }

  async function applyConfig(nextStreaming = streamingActive) {
    saveMessage = null;
    if (ndiSourceName.trim().length === 0) {
      loadError = "Broadcast source name cannot be empty";
      return;
    }
    loadError = null;
    try {
      const updated = await invoke<BroadcastCapabilities>("apply_broadcast_profile", {
        sourceName: ndiSourceName.trim(),
        alphaMode,
        decklinkMode,
        streamingActive: nextStreaming,
      });
      if (updated) caps = updated;
      streamingActive = nextStreaming;
      saveMessage = `Configured "${ndiSourceName.trim()}" (${alphaMode} / ${decklinkMode}${nextStreaming ? " · LIVE" : ""})`;
    } catch (err) {
      loadError = String(err);
    }
  }

  function toggleStream() {
    void applyConfig(!streamingActive);
  }

  onMount(() => {
    void refreshStatus();
  });
</script>

<div class="broadcast-panel" data-testid="broadcast-panel">
  <div class="header">
    <h2>Broadcast Video I/O, Virtual Outputs & LAN Mobile Remote</h2>
    <div class="header-actions">
      <button class="btn" class:live-btn={streamingActive} onclick={toggleStream}>
        {streamingActive ? "⏹ Stop Broadcast Output" : "● Start Broadcast Output"}
      </button>
      <button class="btn" onclick={refreshStatus}>Probe Runtimes</button>
    </div>
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
      <div class="card">
        <h3>LAN Browser Stage Display (Vocalists)</h3>
        <p><code>{caps.web_stage_url}</code></p>
      </div>
      <div class="card">
        <h3>LAN Musician Stage (Chord Charts & Sheet Music)</h3>
        <p><code>{caps.web_musician_url ?? `${caps.web_stage_url.replace(/\/$/, "")}/musician`}</code></p>
      </div>
      <div class="card">
        <h3>LAN Mobile Remote Controller (PIN: {caps.remote_pin ?? "2026"})</h3>
        <p><code>{caps.web_remote_url ?? `${caps.web_stage_url.replace(/\/$/, "")}/remote`}</code></p>
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
    <button class="btn primary" onclick={() => void applyConfig(streamingActive)}>
      Apply Broadcast Profile
    </button>
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
    gap: 8px;
  }

  .header-actions {
    display: flex;
    gap: 8px;
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

  .card h3 {
    margin: 0 0 4px 0;
    font-size: 12px;
    color: var(--text-1);
  }

  .card p {
    margin: 0;
    font-size: 12px;
    word-break: break-all;
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

  .btn.live-btn {
    background: #b91c1c;
    border-color: #ef4444;
    color: #fff;
  }
</style>
