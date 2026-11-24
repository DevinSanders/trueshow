import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import LayerManager from "./LayerManager.svelte";
import type { SceneState } from "./types";

function makeScene(): SceneState {
  return {
    revision: 3,
    blackout: false,
    active_slide: {
      id: "s-1",
      label: "02 — Amazing Grace",
      group: "Verse 1",
      headline: "Amazing Grace",
      body: "How sweet the sound",
      chords_line: null,
      ccli_number: "22025",
      media_path: null,
      primary_color: { r: 0.1, g: 0.2, b: 0.3, a: 1 },
      secondary_color: { r: 0.2, g: 0.3, b: 0.4, a: 1 },
      accent_color: { r: 1, g: 1, b: 1, a: 1 },
      motion_style: "AuroraWave",
      transition: "Cut",
      elements: [],
    },
    next_slide_preview: null,
    active_background: {
      label: "Sanctuary Glow (GPU Motion)",
      media_path: "builtin://motion/sanctuary-glow",
      motion_style: "SanctuaryGlow",
      primary_color: { r: 0.2, g: 0.1, b: 0.05, a: 1 },
      secondary_color: { r: 0.7, g: 0.3, b: 0.1, a: 1 },
      accent_color: { r: 1, g: 0.9, b: 0.6, a: 1 },
    },
    active_foreground_media: {
      label: "SermonGraphic.png",
      media_path: "C:\\Media\\SermonGraphic.png",
      kind: "image",
      target_layer: "Slide",
    },
    video_input_active: false,
    media_layer_active: true,
    slide_layer_active: true,
    announcements_layer_active: false,
    overlay_layer_active: true,
    mask_style: "SoftVignette",
    announcement_text: null,
    stage_message: "Nursery #12",
    timer_readout: "04:30",
    looks: {},
    outputs: [],
    deck: [],
  };
}

describe("LayerManager (7-Layer Stack Inspection, Clearing & Drag-Drop)", () => {
  it("displays active content per layer, clears individual layers, toggles inputs, and accepts media drops", async () => {
    const onClearLayer = vi.fn();
    const onClearAll = vi.fn();
    const onToggleBlackout = vi.fn();
    const onToggleOverlay = vi.fn();
    const onToggleVideoInput = vi.fn();
    const onDropMediaToLayer = vi.fn();

    render(LayerManager, {
      props: {
        scene: makeScene(),
        onClearLayer,
        onClearAll,
        onToggleBlackout,
        onToggleOverlay,
        onToggleVideoInput,
        onDropMediaToLayer,
      },
    });

    expect(screen.getByText("Sanctuary Glow (GPU Motion)")).toBeTruthy();
    expect(screen.getByText(/FG Image\/Media: SermonGraphic\.png/)).toBeTruthy();
    expect(screen.getByText(/04:30 · Nursery #12/)).toBeTruthy();

    await fireEvent.click(screen.getByLabelText("Clear Media / Background"));
    expect(onClearLayer).toHaveBeenCalledWith("MediaBackground");

    await fireEvent.click(screen.getByLabelText("Clear Slide / Foreground"));
    expect(onClearLayer).toHaveBeenCalledWith("Slide");

    await fireEvent.click(screen.getByLabelText("Clear Masks"));
    expect(onClearLayer).toHaveBeenCalledWith("Masks");

    await fireEvent.click(screen.getByLabelText("Toggle Props & Overlays"));
    expect(onToggleOverlay).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByLabelText("Toggle Video Input"));
    expect(onToggleVideoInput).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByText("Clear All"));
    expect(onClearAll).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByText("Blackout"));
    expect(onToggleBlackout).toHaveBeenCalledTimes(1);

    // Drag & drop media onto Background layer row (plus dragLeave & non-droppable layer)
    const bgRow = screen.getByText("Media / Background").closest("li")!;
    await fireEvent.dragOver(bgRow);
    await fireEvent.dragLeave(bgRow);
    await fireEvent.dragOver(bgRow);
    await fireEvent.drop(bgRow, {
      dataTransfer: {
        getData: (t: string) =>
          t === "application/x-openpresenter-media"
            ? JSON.stringify({ blake3_hash: "h-aurora" })
            : "",
      },
    });
    expect(onDropMediaToLayer).toHaveBeenCalledWith("h-aurora", "background");

    // Non-droppable layer (Masks) ignores drag/drop safely
    const maskRow = screen.getByText("Masks").closest("li")!;
    await fireEvent.dragOver(maskRow);
    await fireEvent.drop(maskRow, { dataTransfer: { getData: () => "" } });

    // Malformed drag payload is safely ignored
    await fireEvent.drop(bgRow, {
      dataTransfer: { getData: () => "{not-json" },
    });
    expect(onDropMediaToLayer).toHaveBeenCalledTimes(1);
  });
});
