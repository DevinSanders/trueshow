import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import StageDesignerPanel from "./StageDesignerPanel.svelte";
import type { SceneState } from "./types";

function makeScene(withStage = true): SceneState {
  return {
    revision: 1,
    blackout: false,
    active_slide: null,
    next_slide_preview: null,
    video_input_active: false,
    media_layer_active: true,
    slide_layer_active: true,
    announcements_layer_active: false,
    overlay_layer_active: true,
    mask_style: "None",
    announcement_text: null,
    stage_message: null,
    timer_readout: null,
    looks: {},
    outputs: withStage
      ? [
          {
            id: "00000000-0000-0000-0000-000000000066",
            name: "Stage 1",
            kind: "Stage",
            monitor_index: 1,
            width: 1280,
            height: 720,
            fullscreen: false,
            look_id: "00000000-0000-0000-0000-000000000002",
          },
        ]
      : [],
    deck: [],
  };
}

describe("StageDesignerPanel (Visual Stage Confidence Designer)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("switches confidence & musician presets, creates a separate Musician Stage screen, and applies layout live", async () => {
    const scene = makeScene(true);
    const onSceneChange = vi.fn();
    mockIPC((cmd) => {
      if (cmd === "upsert_output_screen") {
        return {
          state: {
            ...scene,
            outputs: [
              ...scene.outputs,
              {
                id: "00000000-0000-0000-0000-000000000077",
                name: "Musician Chord Chart (2)",
                kind: "Stage",
                monitor_index: null,
                width: 1280,
                height: 720,
                fullscreen: false,
                look_id: "00000000-0000-0000-0000-000000000002",
              },
            ],
          },
        };
      }
      if (cmd === "set_output_stage_layout") return scene;
      return null;
    });

    render(StageDesignerPanel, { props: { scene, onSceneChange } });
    await fireEvent.click(screen.getByRole("button", { name: "Pastor Big Clock" }));
    await fireEvent.click(screen.getByRole("button", { name: "Minimal Lyrics" }));
    await fireEvent.click(screen.getByRole("button", { name: "Band w/ Chords" }));
    await fireEvent.click(screen.getByRole("button", { name: "Musician Chord Chart" }));
    expect(screen.getByLabelText("Lookahead Sections")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Musician Sheet Music" }));
    expect(screen.getByText(/PAGE 1 \(TREBLE \+ BASS CLEF\)/)).toBeTruthy();

    await fireEvent.click(screen.getByRole("button", { name: "+ Separate Musician Screen" }));
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("Musician Chord Chart");
    });

    await fireEvent.click(screen.getByRole("button", { name: "Apply Stage Layout Live" }));
    await waitFor(() => {
      expect(onSceneChange).toHaveBeenCalled();
      expect(screen.getByRole("status").textContent).toContain("Stage Display");
    });
  });

  it("shows validation alert when no Stage Display is configured", async () => {
    const scene = makeScene(false);
    const onSceneChange = vi.fn();
    render(StageDesignerPanel, { props: { scene, onSceneChange } });
    await fireEvent.click(screen.getByRole("button", { name: "Apply Stage Layout Live" }));
    expect(screen.getByRole("alert").textContent).toContain("Create a Stage Display");
  });
});
