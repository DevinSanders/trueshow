import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import PreviewCanvas from "./PreviewCanvas.svelte";

function makeValidFrame(width: number, height: number): ArrayBuffer {
  const buf = new ArrayBuffer(8 + width * height * 4);
  const dv = new DataView(buf);
  dv.setUint32(0, width, true);
  dv.setUint32(4, height, true);
  new Uint8Array(buf, 8).fill(255);
  return buf;
}

describe("PreviewCanvas backpressured binary RGBA8 preview polling", () => {
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

  it("renders valid binary RGBA8 frames and safely ignores empty, truncated, or zero-sized payloads", async () => {
    let callIdx = 0;
    const frames: ArrayBuffer[] = [
      new ArrayBuffer(0), // no new frame yet
      new ArrayBuffer(4), // truncated header
      new ArrayBuffer(12), // zero width/height
      makeValidFrame(2, 2), // valid frame #1
      makeValidFrame(2, 2), // valid frame #2 (triggers FPS window update)
    ];

    let nowValue = 1000;
    vi.spyOn(performance, "now").mockImplementation(() => {
      nowValue += callIdx >= 4 ? 1200 : 10;
      return nowValue;
    });

    mockIPC((cmd) => {
      if (cmd === "poll_preview_frame") {
        const frame = frames[Math.min(callIdx, frames.length - 1)];
        callIdx += 1;
        return frame;
      }
      return null;
    });

    const { unmount } = render(PreviewCanvas, {
      props: {
        outputId: "out-1",
        label: "Audience 1",
        kind: "Audience",
        connected: true,
        enginePid: 1001,
      },
    });

    await waitFor(() => expect(screen.getByText(/2×2/)).toBeTruthy(), { timeout: 2000 });
    await waitFor(() => expect(putImageData.mock.calls.length).toBeGreaterThanOrEqual(2), {
      timeout: 2000,
    });

    unmount();
  });

  it("handles preview poll failures gracefully with warning and backoff", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockIPC((cmd) => {
      if (cmd === "poll_preview_frame") {
        throw new Error("shm ring missing");
      }
      return null;
    });

    const { unmount } = render(PreviewCanvas, {
      props: {
        outputId: "out-missing",
        label: "Stage 1",
        kind: "Stage",
        connected: true,
      },
    });

    await waitFor(() => expect(warnSpy).toHaveBeenCalled());
    unmount();
  });
});
