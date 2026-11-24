import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import QuickLyricsPanel from "./QuickLyricsPanel.svelte";
import type { SceneState } from "./types";

function makeScene(): SceneState {
  const mk = (id: string, group: string) => ({
    id,
    label: group,
    group,
    headline: group,
    body: group,
    chords_line: null,
    ccli_number: null,
    media_path: null,
    primary_color: { r: 0, g: 0, b: 0, a: 1 },
    secondary_color: { r: 0, g: 0, b: 0, a: 1 },
    accent_color: { r: 1, g: 1, b: 1, a: 1 },
    motion_style: "AuroraWave",
    transition: "Cut" as const,
    elements: [],
  });
  const s1 = mk("s1", "Verse 1");
  const s2 = mk("s2", "Chorus");
  return {
    revision: 1,
    blackout: false,
    active_slide: s1,
    next_slide_preview: s2,
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
    outputs: [],
    deck: [s1, s2],
    master_deck: [s1, s2],
    active_arrangements: [{ id: "arr-1", name: "Default", group_order: ["Verse 1", "Chorus"] }],
    active_arrangement_id: "arr-1",
  };
}

describe("QuickLyricsPanel (Quick Lyrics Reflow & Arrangements)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("parses quick lyrics, builds custom arrangements, and switches live order", async () => {
    const scene = makeScene();
    const onSceneChange = vi.fn();
    mockIPC((cmd, args) => {
      const p = (args ?? {}) as Record<string, unknown>;
      if (cmd === "parse_and_save_quick_lyrics") {
        return p.loadToDeck ? scene : null;
      }
      if (cmd === "select_arrangement" || cmd === "save_arrangements") {
        return scene;
      }
      return null;
    });

    render(QuickLyricsPanel, { props: { scene, onSceneChange } });

    // Validation path: empty title
    await fireEvent.input(screen.getByLabelText("Quick Lyrics Title"), { target: { value: "  " } });
    await fireEvent.click(screen.getByRole("button", { name: "Parse & Load to Cue Deck" }));
    expect(screen.getByRole("alert").textContent).toContain("song title");

    // Happy path: parse & load + save only
    await fireEvent.input(screen.getByLabelText("Quick Lyrics Title"), {
      target: { value: "Holy Song" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Parse & Load to Cue Deck" }));
    await waitFor(() => expect(onSceneChange).toHaveBeenCalled());

    await fireEvent.click(screen.getByRole("button", { name: "Save to Library Only" }));
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("Saved");
    });

    // Arrangement validation then build sequence & save
    await fireEvent.click(screen.getByRole("button", { name: "Save & Activate Arrangement" }));
    expect(screen.getByRole("alert").textContent).toContain("arrangement name");

    await fireEvent.click(screen.getByRole("button", { name: "+ Verse 1" }));
    await fireEvent.click(screen.getByRole("button", { name: "+ Chorus" }));
    await fireEvent.click(screen.getByRole("button", { name: "2. Chorus ×" }));
    await fireEvent.click(screen.getByRole("button", { name: "+ Chorus" }));
    await fireEvent.click(screen.getByRole("button", { name: "Save & Activate Arrangement" }));
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("Full Band");
    });

    await fireEvent.click(screen.getByRole("button", { name: "Master Order" }));
    await fireEvent.click(screen.getByRole("button", { name: "Default (2)" }));
  });
});
