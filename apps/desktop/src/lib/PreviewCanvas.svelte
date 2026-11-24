<script lang="ts">
  import { Channel, invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";

  interface Props {
    outputId: string;
    label: string;
    kind: "Audience" | "Stage";
    connected: boolean;
  }

  let { outputId, label, kind, connected }: Props = $props();

  let canvasEl: HTMLCanvasElement | undefined = $state();
  let streamFps = $state(0);
  let frameRes = $state("waiting...");
  let stopStream = false;

  let frameCounter = 0;
  let lastFpsStamp = performance.now();

  $effect(() => {
    if (!connected || !canvasEl) return;

    stopStream = false;
    const canvas = canvasEl;
    const ctx2d = canvas.getContext("2d", { alpha: true });
    let imageData: ImageData | null = null;

    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (payload) => {
      if (stopStream || !ctx2d || !(payload instanceof ArrayBuffer) || payload.byteLength < 8) {
        return;
      }

      const header = new DataView(payload, 0, 8);
      const width = header.getUint32(0, true);
      const height = header.getUint32(4, true);
      const expectedBytes = width * height * 4;
      if (width === 0 || height === 0 || payload.byteLength < 8 + expectedBytes) {
        return;
      }

      if (canvas.width !== width || canvas.height !== height || !imageData) {
        canvas.width = width;
        canvas.height = height;
        imageData = ctx2d.createImageData(width, height);
        frameRes = `${width}×${height}`;
      }

      const pixels = new Uint8ClampedArray(payload, 8, expectedBytes);
      imageData.data.set(pixels);
      ctx2d.putImageData(imageData, 0, 0);

      frameCounter += 1;
      const now = performance.now();
      if (now - lastFpsStamp >= 1000) {
        streamFps = Math.round((frameCounter * 1000) / (now - lastFpsStamp));
        frameCounter = 0;
        lastFpsStamp = now;
      }
    };

    invoke("start_preview_stream", { outputId, onFrame: channel }).catch((err) => {
      console.warn(`preview stream failed for ${outputId}:`, err);
    });

    return () => {
      stopStream = true;
    };
  });

  onDestroy(() => {
    stopStream = true;
  });
</script>

<div class="monitor-card">
  <div class="monitor-header">
    <span class="badge" class:stage={kind === "Stage"}>{kind.toUpperCase()}</span>
    <span class="title">{label}</span>
    <span class="meta">{frameRes} · {streamFps} fps</span>
  </div>
  <div class="viewport">
    <canvas bind:this={canvasEl}></canvas>
  </div>
</div>

<style>
  .monitor-card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .monitor-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }

  .badge {
    padding: 2px 6px;
    border-radius: 4px;
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.05em;
    background: #1d4ed8;
    color: #fff;
  }

  .badge.stage {
    background: #0f766e;
  }

  .title {
    font-weight: 600;
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta {
    color: var(--text-1);
    font-variant-numeric: tabular-nums;
  }

  .viewport {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    background:
      linear-gradient(45deg, #121722 25%, transparent 25%),
      linear-gradient(-45deg, #121722 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, #121722 75%),
      linear-gradient(-45deg, transparent 75%, #121722 75%);
    background-size: 16px 16px;
    background-position: 0 0, 0 8px, 8px -8px, -8px 0;
    background-color: #0b0e15;
  }

  canvas {
    width: 100%;
    height: 100%;
    display: block;
    object-fit: contain;
  }
</style>
