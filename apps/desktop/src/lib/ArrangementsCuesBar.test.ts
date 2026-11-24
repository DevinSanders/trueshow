import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import ArrangementsCuesBar from "./ArrangementsCuesBar.svelte";
import type { Arrangement, AutoPlayTrack, SceneState, Slide } from "./types";

function makeSlide(id: string, label: string, group: string, autoAdvance?: number | null): Slide {
  return {
    id,
    label,
    group,
    headline: label,
    body: `${group} lyrics`,
    chords_line: null,
    ccli_number: "22025",
    media_path: null,
    auto_advance_secs: autoAdvance ?? null,
    loop_to_first: false,
    primary_color: { r: 0.1, g: 0.2, b: 0.3, a: 1 },
    secondary_color: { r: 0.2, g: 0.3, b: 0.4, a: 1 },
    accent_color: { r: 0.9, g: 0.7, b: 0.2, a: 1 },
    motion_style: "AuroraWave",
    transition: { Dissolve: { duration_ms: 450 } },
    elements: [],
  };
}

function makeScene(): SceneState {
  const s1 = makeSlide("00000000-0000-0000-0000-000000000001", "01 — Verse 1", "Verse 1", 4.0);
  const s2 = makeSlide("00000000-0000-0000-0000-000000000002", "02 — Chorus", "Chorus", 6.0);
  const s3 = makeSlide("00000000-0000-0000-0000-000000000003", "03 — Bridge", "Bridge", null);
  return {
    revision: 1,
    blackout: false,
    active_slide: s1,
    next_slide_preview: s2,
    video_input_active: false,
    media_layer_active: true,
    slide_layer_active: true,
    announcements_layer_active: false,
    overlay_layer_active: false,
    mask_style: "None",
    announcement_text: null,
    stage_message: null,
    timer_readout: null,
    looks: {},
    outputs: [],
    deck: [s1, s2, s3],
    master_deck: [s1, s2, s3],
    active_arrangements: [
      {
        id: "arr-sun",
        name: "Sunday Standard",
        group_order: ["Verse 1", "Chorus", "Bridge", "Chorus"],
      },
    ],
    active_arrangement_id: "arr-sun",
    active_cue_tracks: [],
  };
}

describe("ArrangementsCuesBar (ProPresenter Drag+Drop Arrangements & Live Cue Recorder)", () => {
  afterEach(() => {
    clearMocks();
    vi.useRealTimers();
  });

  it("creates, switches, duplicates, deletes, and drag+drop copies/reorders arrangements", async () => {
    let current = makeScene();
    const onSceneChange = vi.fn((next: SceneState) => {
      current = next;
    });
    const onTriggerSlide = vi.fn();
    const onError = vi.fn();

    mockIPC((cmd, args) => {
      const p = (args ?? {}) as Record<string, unknown>;
      if (cmd === "select_arrangement") {
        current = {
          ...current,
          active_arrangement_id: (p.arrangementId as string | null) ?? null,
        };
        return structuredClone(current);
      }
      if (cmd === "save_arrangements") {
        current = {
          ...current,
          active_arrangements: p.arrangements as Arrangement[],
          active_arrangement_id: (p.activeId as string | null) ?? null,
        };
        return structuredClone(current);
      }
      return null;
    });

    const { rerender } = render(ArrangementsCuesBar, {
      props: { scene: current, onSceneChange, onTriggerSlide, onError },
    });

    // Open Arrangements Editor
    await fireEvent.click(screen.getByRole("button", { name: "Arrangements Editor" }));
    expect(screen.getByTestId("arrangement-editor")).toBeTruthy();

    // Drag "Chorus" from Master palette and drop at end of arrangement track (copies Chorus again!)
    const masterChorus = screen
      .getByTestId("master-group-palette")
      .querySelector("button:nth-child(2)")!;
    await fireEvent.dragStart(masterChorus);
    const dropzone = screen.getByTestId("arrangement-dropzone");
    await fireEvent.dragOver(dropzone);
    await fireEvent.drop(dropzone, {
      dataTransfer: {
        getData: (t: string) => (t === "application/x-openpresenter-master-group" ? "Chorus" : ""),
      },
    });
    await waitFor(() => expect(current.active_arrangements?.[0].group_order.length).toBe(5));
    await rerender({ scene: current, onSceneChange, onTriggerSlide, onError });

    // Drag slot 2 ("Bridge") before slot 0 ("Verse 1") to reorder
    const trackTokens = dropzone.querySelectorAll(".track-token");
    await fireEvent.dragStart(trackTokens[2]);
    await fireEvent.dragOver(trackTokens[0]);
    await fireEvent.drop(trackTokens[0], {
      dataTransfer: {
        getData: (t: string) => (t === "application/x-openpresenter-arr-idx" ? "2" : ""),
      },
    });
    await waitFor(() => expect(current.active_arrangements?.[0].group_order[0]).toBe("Bridge"));
    await rerender({ scene: current, onSceneChange, onTriggerSlide, onError });

    // Remove slot 1 via × button, duplicate, create new, switch to Master, and delete
    await fireEvent.click(screen.getByLabelText(/Remove .* at slot 1/));
    await waitFor(() => expect(current.active_arrangements?.[0].group_order.length).toBe(4));
    await rerender({ scene: current, onSceneChange, onTriggerSlide, onError });

    await fireEvent.click(screen.getByRole("button", { name: "Duplicate" }));
    await waitFor(() => expect(current.active_arrangements?.length).toBe(2));
    await rerender({ scene: current, onSceneChange, onTriggerSlide, onError });

    await fireEvent.input(screen.getByLabelText("New Arrangement Name"), {
      target: { value: "Acoustic Short" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "+ New" }));
    await waitFor(() => expect(current.active_arrangements?.length).toBe(3));
    await rerender({ scene: current, onSceneChange, onTriggerSlide, onError });

    await fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(current.active_arrangements?.length).toBe(2));
    await rerender({ scene: current, onSceneChange, onTriggerSlide, onError });

    // Switch back to Master via dropdown
    await fireEvent.change(screen.getByLabelText("Active Arrangement"), { target: { value: "" } });
    await waitFor(() => expect(current.active_arrangement_id).toBeNull());
  });

  it("records live slide cues, saves AutoPlayTrack, edits per-slide timers, and runs Auto-Play", async () => {
    vi.useFakeTimers();
    let current = makeScene();
    const onSceneChange = vi.fn((next: SceneState) => {
      current = next;
    });
    const onTriggerSlide = vi.fn((id: string) => {
      const found = current.deck.find((s) => s.id === id) ?? null;
      current = { ...current, active_slide: found };
    });
    const onError = vi.fn();

    mockIPC((cmd, args) => {
      const p = (args ?? {}) as Record<string, unknown>;
      if (cmd === "apply_recorded_cues") {
        const track = p.cueTrack as AutoPlayTrack | null;
        current = {
          ...current,
          active_cue_tracks: track ? [...(current.active_cue_tracks ?? []), track] : current.active_cue_tracks,
        };
        return structuredClone(current);
      }
      if (cmd === "set_slide_automation") {
        const sid = String(p.slideId);
        const secs = p.autoAdvanceSecs as number | null;
        current = {
          ...current,
          deck: current.deck.map((s) => (s.id === sid ? { ...s, auto_advance_secs: secs } : s)),
        };
        return structuredClone(current);
      }
      if (cmd === "delete_cue_track") {
        const tid = String(p.trackId);
        current = {
          ...current,
          active_cue_tracks: (current.active_cue_tracks ?? []).filter((t) => t.id !== tid),
        };
        return structuredClone(current);
      }
      return null;
    });

    const { rerender } = render(ArrangementsCuesBar, {
      props: {
        scene: current,
        editingTimerSlideId: current.deck[0].id,
        onSceneChange,
        onTriggerSlide,
        onError,
      },
    });

    // Panel auto-opens when editingTimerSlideId is set
    expect(screen.getByTestId("autoplay-panel")).toBeTruthy();

    // Save per-slide timer & clear all timers
    await fireEvent.input(screen.getByLabelText("Slide Auto-Advance Seconds"), {
      target: { value: "7.5" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save Slide Timer" }));
    await waitFor(() => expect(current.deck[0].auto_advance_secs).toBe(7.5));
    await fireEvent.click(screen.getByRole("button", { name: "Clear Slide Timer" }));
    await fireEvent.click(screen.getByRole("button", { name: "Clear All Timers" }));

    // Record live cues across 2 slides and save recorded AutoPlayTrack
    await fireEvent.click(screen.getByRole("button", { name: "● Record Cues" }));
    expect(screen.getByTestId("rec-indicator")).toBeTruthy();
    vi.advanceTimersByTime(1500);

    // Advance active_slide while recording
    current = { ...current, active_slide: current.deck[1] };
    await rerender({ scene: current, onSceneChange, onTriggerSlide, onError });
    vi.advanceTimersByTime(1200);

    await fireEvent.click(screen.getByRole("button", { name: "■ Stop & Save Rec" }));
    await waitFor(() => expect(current.active_cue_tracks?.length).toBe(1));
    await rerender({ scene: current, onSceneChange, onTriggerSlide, onError });

    // Start Auto-Play on the recorded track and verify slide advances automatically
    await fireEvent.click(screen.getByRole("button", { name: "▶ Auto-Play" }));
    expect(screen.getByTestId("autoplay-indicator")).toBeTruthy();
    vi.advanceTimersByTime(2000);
    expect(onTriggerSlide).toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: "■ Stop Auto-Play" }));
    await fireEvent.click(screen.getByRole("button", { name: "Delete Track" }));
    await waitFor(() => expect(current.active_cue_tracks?.length).toBe(0));
  });
});
