<script lang="ts">
  import type { EngineSnapshot, EngineTelemetry } from "./types";

  interface Props {
    connected: boolean;
    statusMessage: string;
    snapshot: EngineSnapshot | null;
    telemetry: EngineTelemetry | null;
    crashCount: number;
    onReconnect: () => void;
    onOpenDiagnostics: () => void;
  }

  let {
    connected,
    statusMessage,
    snapshot,
    telemetry,
    crashCount,
    onReconnect,
    onOpenDiagnostics,
  }: Props = $props();

  const outputCount = $derived(snapshot?.state.outputs.length ?? 0);
  const frozen = $derived(Boolean(snapshot?.state.freeze_output));
  const blackout = $derived(Boolean(snapshot?.state.blackout));
</script>

<footer class="statusbar" data-testid="status-bar">
  <div class="sb-group">
    <span class="sb-conn">
      <span class="sb-dot" class:live={connected}></span>
      <span>{connected ? "Connected" : statusMessage}</span>
    </span>
    {#if !connected}
      <button type="button" class="sb-link" onclick={onReconnect}>Reconnect</button>
    {/if}
    {#if snapshot}
      <span class="sb-sep">·</span>
      <span>{outputCount} {outputCount === 1 ? "output" : "outputs"}</span>
      {#if blackout}
        <span class="sb-badge danger">BLACKOUT</span>
      {:else if frozen}
        <span class="sb-badge warn">FROZEN</span>
      {/if}
    {/if}
  </div>

  <div class="sb-group sb-metrics">
    {#if snapshot}
      <span class="sb-gpu" title={`${snapshot.gpu_adapter_name} (${snapshot.gpu_backend})`}>
        GPU: <strong>{snapshot.gpu_adapter_name}</strong> ({snapshot.gpu_backend})
      </span>
      <span class="sb-sep">·</span>
      <span>FPS: <strong>{telemetry ? telemetry.fps.toFixed(1) : "—"}</strong></span>
      <span class="sb-sep">·</span>
      <span>p99: <strong>{telemetry ? `${telemetry.p99_frame_ms.toFixed(1)}ms` : "—"}</strong></span>
      <span class="sb-sep">·</span>
      <span>PID {snapshot.engine_pid}</span>
      <span class="sb-sep">·</span>
    {/if}
    <button
      type="button"
      class="sb-link"
      class:warn={crashCount > 0}
      onclick={onOpenDiagnostics}
    >
      {crashCount > 0 ? `Diagnostics (${crashCount})` : "Diagnostics"}
    </button>
  </div>
</footer>

<style>
  .statusbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    height: 24px;
    padding: 0 10px;
    background: var(--bg-1);
    border-top: 1px solid var(--border);
    color: var(--text-1);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
    overflow: hidden;
    user-select: none;
  }

  .sb-group {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    white-space: nowrap;
  }

  .sb-conn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text-0);
  }

  .sb-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--warn);
    flex-shrink: 0;
  }

  .sb-dot.live {
    background: var(--ok);
    box-shadow: 0 0 6px var(--ok);
  }

  .sb-sep {
    opacity: 0.45;
  }

  .sb-metrics strong {
    color: var(--text-0);
    font-weight: 600;
  }

  .sb-gpu {
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sb-badge {
    padding: 0 5px;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  .sb-badge.danger {
    background: rgba(239, 68, 68, 0.25);
    color: #fecaca;
  }

  .sb-badge.warn {
    background: rgba(245, 158, 11, 0.25);
    color: #fde68a;
  }

  .sb-link {
    border: none;
    background: transparent;
    color: var(--text-1);
    padding: 0 4px;
    font-size: 11px;
    border-radius: 3px;
  }

  .sb-link:hover {
    background: var(--bg-2);
    color: var(--text-0);
  }

  .sb-link.warn {
    color: #fde68a;
    font-weight: 600;
  }
</style>
