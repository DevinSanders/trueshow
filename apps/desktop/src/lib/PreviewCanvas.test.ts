import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import PreviewCanvas from "./PreviewCanvas.svelte";

interface MockChannel {
  onmessage?: (payload: ArrayBuffer) => void;
}

describe("PreviewCanvas binary RGBA8 channel streaming", () => {
  const putImageData = vi.fn();

  beforeEach(() => {
    putImageData.mockReset();
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      createImageData: (w: number, h: number) => ({
        width: w,
        height: h,
        data: new Uint8ClampedArray(w * h * 4),
      }),
      putImageData,
    } as unknown as CanvasRenderingContext2D);
  });

  afterEach(() => {
    clearMocks();
    vi.restoreAllMocks();
  });

  it("renders valid binary RGBA8 frames and safely ignores truncated or zero-sized payloads", async () => {
    let capturedChannel: MockChannel | null = null;
    mockIPC((cmd, args) => {
      if (cmd === "start_preview_stream") {
        const a = args as { onFrame?: MockChannel };
        capturedChannel = a.onFrame ?? null;
        return null;
      }
      return null;
    });

    const { unmount } = render(PreviewCanvas, {
      props: {
        outputId: "out-1",
        label: "Audience 1",
        kind: "Audience",
        connected: true,
      },
    });

    await waitFor(() => expect(capturedChannel).not.toBeNull());
    const onMessage = capturedChannel!.onmessage!;

    // Failure paths: truncated header and zero-dimension payload are ignored cleanly
    onMessage(new ArrayBuffer(4));
    const zeroHeader = new ArrayBuffer(12);
    onMessage(zeroHeader);
    expect(putImageData).not.toHaveBeenCalled();

    // Happy path: valid 2x2 RGBA8 frame packet (8-byte header + 16 pixel bytes)
    const buf = new ArrayBuffer(8 + 2 * 2 * 4);
    const dv = new DataView(buf);
    dv.setUint32(0, 2, true);
    dv.setUint32(4, 2, true);
    new Uint8Array(buf, 8).fill(255);
    onMessage(buf);

    await waitFor(() => expect(screen.getByText(/2×2/)).toBeTruthy());
    expect(putImageData).toHaveBeenCalledTimes(1);

    // Advance clock >= 1000ms to exercise FPS calculation branch
    const nowSpy = vi.spyOn(performance, "now").mockReturnValue(performance.now() + 1500);
    onMessage(buf);
    nowSpy.mockRestore();

    unmount();
  });

  it("handles preview stream startup failure gracefully", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockIPC((cmd) => {
      if (cmd === "start_preview_stream") {
        throw new Error("shm ring missing");
      }
      return null;
    });

    render(PreviewCanvas, {
      props: {
        outputId: "out-missing",
        label: "Stage 1",
        kind: "Stage",
        connected: true,
      },
    });

    await waitFor(() => expect(warnSpy).toHaveBeenCalled());
  });
});
