import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App.svelte";
import type { EngineSnapshot, SceneState, SongRecord } from "./lib/types";

function makeFixtureSnapshot(): EngineSnapshot {
  const slide1 = {
    id: "00000000-0000-0000-0000-0000000003e9",
    label: "01 — Prelude Welcome",
    group: "Pre-Service",
    headline: "WELCOME TO WORSHIP",
    body: "Sunday Morning Gathering",
    chords_line: null,
    ccli_number: null,
    media_path: null,
    primary_color: { r: 0.06, g: 0.14, b: 0.28, a: 1 },
    secondary_color: { r: 0.08, g: 0.42, b: 0.56, a: 1 },
    accent_color: { r: 0.95, g: 0.78, b: 0.35, a: 1 },
    motion_style: "AuroraWave",
    transition: { Dissolve: { duration_ms: 500 } },
    elements: [
      {
        id: "00000000-0000-0000-0000-000000001389",
        name: "Primary Lyrics",
        bounds: { x: 160, y: 250, width: 1600, height: 480 },
        opacity: 1,
        z_index: 10,
        kind: {
          TextBox: {
            text: "WELCOME TO WORSHIP",
            style: {
              font_family: "sans-serif",
              font_size: 86,
              bold: true,
              italic: false,
              line_height: 1.22,
              color: { r: 1, g: 1, b: 1, a: 1 },
              stroke_color: { r: 0, g: 0, b: 0, a: 0.9 },
              stroke_width: 4,
              shadow_color: { r: 0, g: 0, b: 0, a: 0.7 },
              shadow_offset_x: 0,
              shadow_offset_y: 6,
              shadow_blur: 10,
              backdrop_color: { r: 0, g: 0, b: 0, a: 0 },
              backdrop_radius: 18,
              backdrop_padding: 24,
              h_align: "Center" as const,
              v_align: "Middle" as const,
              auto_shrink: true,
            },
          },
        },
      },
    ],
  };

  const slide2 = {
    ...slide1,
    id: "00000000-0000-0000-0000-0000000003ea",
    label: "02 — Amazing Grace",
    group: "Verse 1",
    headline: "Amazing grace! How sweet the sound",
    body: "That saved a wretch like me",
    ccli_number: "22025",
  };

  return {
    protocol_version: 1,
    engine_pid: 4242,
    gpu_adapter_name: "Test GPU D3D12",
    gpu_backend: "Dx12",
    displays: [],
    preview_buses: [],
    telemetry: {
      fps: 60.0,
      avg_frame_ms: 16.4,
      p99_frame_ms: 16.6,
      total_frames: 120,
      dropped_frames: 0,
      outputs: [],
    },
    state: {
      revision: 1,
      blackout: false,
      active_slide: slide1,
      next_slide_preview: slide2,
      active_audio: {
        title: "Walk-In Piano",
        media_path: "walkin.mp3",
        playing: true,
        looping: true,
        volume: 0.8,
      },
      video_input_active: false,
      media_layer_active: true,
      slide_layer_active: true,
      announcements_layer_active: false,
      overlay_layer_active: true,
      mask_style: "None",
      announcement_text: null,
      stage_message: null,
      timer_readout: "05:00",
      looks: {
        "look-1": {
          id: "look-1",
          name: "Audience Full",
          enabled_layers: ["MediaBackground", "Slide", "PropsAndOverlays"],
          lower_third_mode: false,
          transparent_background: false,
          slide_blend_mode: "Normal",
        },
        "look-2": {
          id: "look-2",
          name: "Broadcast Lower Third",
          enabled_layers: ["Slide"],
          lower_third_mode: true,
          transparent_background: true,
          slide_blend_mode: "Normal",
        },
      },
      outputs: [
        {
          id: "out-1",
          name: "Audience 1",
          kind: "Audience",
          monitor_index: null,
          width: 1280,
          height: 720,
          fullscreen: false,
          look_id: "look-1",
        },
      ],
      deck: [slide1, slide2],
    },
  };
}

const sampleSongs: SongRecord[] = [
  {
    id: "song-1",
    title: "Amazing Grace",
    author: "John Newton",
    copyright: "Public Domain",
    ccli_number: "22025",
    key_signature: "G",
    bpm: 72,
    lyrics_plain: "Amazing grace! How sweet the sound",
    slides: [],
    updated_at: "2026-11-24T00:00:00Z",
  },
];

describe("OpenPresenter Operator Control Surface UI (Happy & Failure Paths)", () => {
  beforeEach(() => {
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      createImageData: (w: number, h: number) => ({
        width: w,
        height: h,
        data: new Uint8ClampedArray(w * h * 4),
      }),
      putImageData: vi.fn(),
    } as unknown as CanvasRenderingContext2D);
  });

  afterEach(() => {
    clearMocks();
    vi.restoreAllMocks();
  });

  it("connects to engine, triggers slides, toggles blackout, switches looks, sends stage msg, and edits slides (happy path)", async () => {
    const snap = makeFixtureSnapshot();
    let currentState: SceneState = structuredClone(snap.state);

    mockIPC((cmd, args) => {
      const payload = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "connect_engine":
          return { ...snap, state: structuredClone(currentState) };
        case "get_telemetry":
          return snap.telemetry;
        case "list_libraries":
          return [{ id: "default", name: "Default Library", updated_at: "2026-11-24T00:00:00Z" }];
        case "list_playlists":
          return [
            {
              id: "pl-1",
              name: "Sunday Morning",
              item_song_ids: ["song-1"],
              updated_at: "2026-11-24T00:00:00Z",
            },
          ];
        case "list_media_bins":
          return [
            {
              id: "backgrounds",
              name: "Motion Backgrounds",
              category: "background",
              updated_at: "2026-11-24T00:00:00Z",
            },
          ];
        case "list_media_assets":
          return [
            {
              blake3_hash: "h-bg-1",
              bin_id: "backgrounds",
              file_name: "Aurora Wave (GPU Motion)",
              local_path: "builtin://motion/aurora-wave",
              kind: "background",
              width: 1920,
              height: 1080,
              duration_ms: null,
              has_alpha: false,
              size_bytes: 0,
            },
          ];
        case "load_presentation_to_deck":
        case "trigger_media_asset":
        case "toggle_video_input":
          return structuredClone(currentState);
        case "attach_media_to_slide": {
          const sid = String(payload.slideId);
          const hash = payload.blake3Hash as string | null;
          currentState = {
            ...currentState,
            deck: currentState.deck.map((s) =>
              s.id === sid
                ? {
                    ...s,
                    media_cue: hash
                      ? {
                          label: "Aurora Wave",
                          media_path: "builtin://motion/aurora-wave",
                          kind: "background",
                          role: "Background",
                          motion_style: "AuroraWave",
                          primary_color: { r: 0, g: 0, b: 0, a: 1 },
                          secondary_color: { r: 0, g: 0, b: 0, a: 1 },
                          accent_color: { r: 1, g: 1, b: 1, a: 1 },
                        }
                      : null,
                  }
                : s,
            ),
          };
          return structuredClone(currentState);
        }
        case "search_library_songs":
          return sampleSongs;
        case "poll_preview_frame":
          return new ArrayBuffer(0);
        case "trigger_slide": {
          const found = currentState.deck.find((s) => s.id === payload.slideId) ?? null;
          currentState = { ...currentState, active_slide: found, blackout: false };
          return structuredClone(currentState);
        }
        case "set_blackout":
          currentState = { ...currentState, blackout: Boolean(payload.enabled) };
          return structuredClone(currentState);
        case "set_freeze":
          currentState = { ...currentState, freeze_output: Boolean(payload.enabled) };
          return structuredClone(currentState);
        case "undo_scene":
        case "redo_scene":
        case "set_media_transport":
        case "set_audio_playback":
          return structuredClone(currentState);
        case "list_bible_translations":
          return ["KJV", "WEB"];
        case "lookup_scripture":
          return { primary: [], secondary: [] };
        case "toggle_overlay":
          currentState = { ...currentState, overlay_layer_active: Boolean(payload.active) };
          return structuredClone(currentState);
        case "clear_layer":
          currentState = { ...currentState, slide_layer_active: false };
          return structuredClone(currentState);
        case "clear_all":
          currentState = {
            ...currentState,
            media_layer_active: false,
            slide_layer_active: false,
            overlay_layer_active: false,
          };
          return structuredClone(currentState);
        case "set_mask":
          currentState = { ...currentState, mask_style: payload.mask as SceneState["mask_style"] };
          return structuredClone(currentState);
        case "set_stage_message":
          currentState = {
            ...currentState,
            stage_message: (payload.message as string | null) ?? null,
          };
          return structuredClone(currentState);
        case "set_output_look":
          currentState = {
            ...currentState,
            outputs: currentState.outputs.map((o) =>
              o.id === payload.outputId ? { ...o, look_id: String(payload.lookId) } : o,
            ),
          };
          return structuredClone(currentState);
        case "update_slide_element": {
          const el = payload.element as SceneState["deck"][0]["elements"][0];
          currentState = {
            ...currentState,
            active_slide: currentState.active_slide
              ? { ...currentState.active_slide, elements: [el] }
              : null,
          };
          return structuredClone(currentState);
        }
        case "get_diagnostics_status":
          return {
            log_dir: "/tmp/logs",
            crash_dir: "/tmp/crashes",
            engine_log_path: "/tmp/logs/engine.log",
            desktop_log_path: "/tmp/logs/desktop.log",
            active_log_level: "info",
            crash_count: 0,
          };
        case "list_crash_reports":
          return [];
        case "read_diagnostic_logs":
          return ["INFO ok"];
        case "report_frontend_crash":
          return null;
        case "shutdown_engine":
        case "shutdown_all":
          return null;
        default:
          return null;
      }
    });

    render(App);

    // 1. Engine attaches and displays GPU + offline library & media drawers
    await waitFor(() => expect(screen.getByText(/Test GPU D3D12/)).toBeTruthy());
    await waitFor(() => expect(screen.getByText("Amazing Grace")).toBeTruthy());
    expect(screen.getByTestId("library-playlist-drawer")).toBeTruthy();
    expect(screen.getByTestId("media-bin-drawer")).toBeTruthy();

    // Toggle collapsible Libraries Drawer and Media Bin Drawer closed and open
    await fireEvent.click(screen.getByText("Libraries Drawer"));
    expect(screen.queryByTestId("library-playlist-drawer")).toBeNull();
    await fireEvent.click(screen.getByText("Libraries Drawer"));
    expect(screen.getByTestId("library-playlist-drawer")).toBeTruthy();

    await fireEvent.click(screen.getByText("Media Bin"));
    expect(screen.queryByTestId("media-bin-drawer")).toBeNull();
    await fireEvent.click(screen.getByText("Media Bin"));
    expect(screen.getByTestId("media-bin-drawer")).toBeTruthy();

    // Load presentation from Libraries drawer and trigger a background from Media Bin drawer
    await waitFor(() => expect(screen.getByText("Amazing Grace")).toBeTruthy());
    await fireEvent.click(screen.getByText("Amazing Grace"));
    await waitFor(() => expect(screen.getByText("Aurora Wave (GPU Motion)")).toBeTruthy());
    await fireEvent.click(screen.getByText("Aurora Wave (GPU Motion)"));

    // 2. Trigger slide 2
    await fireEvent.click(screen.getByText("02 — Amazing Grace"));
    await waitFor(() => expect(currentState.active_slide?.label).toBe("02 — Amazing Grace"));

    // 3. Toggle blackout ON and OFF
    await fireEvent.click(screen.getByText("BLACKOUT"));
    await waitFor(() => expect(screen.getByText("BLACKOUT ACTIVE")).toBeTruthy());
    await fireEvent.click(screen.getByText("BLACKOUT ACTIVE"));
    await waitFor(() => expect(screen.getByText("BLACKOUT")).toBeTruthy());

    // 4. Toggle overlay, clear slide, clear all, change mask, change look, send stage message, drop media on slide & layer
    await fireEvent.click(screen.getByLabelText("Toggle Props & Overlays"));
    await fireEvent.click(screen.getByLabelText("Clear Slide / Foreground"));
    await fireEvent.click(screen.getByText("Clear All"));
    await fireEvent.click(screen.getByLabelText("Toggle Video Input"));
    await fireEvent.click(screen.getByText("Layers"));
    expect(screen.queryByTestId("layer-manager")).toBeNull();
    await fireEvent.click(screen.getByText("Layers"));
    expect(screen.getByTestId("layer-manager")).toBeTruthy();
    const bgLayerRow = screen.getByText("Media / Background").closest("li")!;
    await fireEvent.drop(bgLayerRow, {
      dataTransfer: {
        getData: (t: string) =>
          t === "application/x-openpresenter-media"
            ? JSON.stringify({ blake3_hash: "h-bg-1" })
            : "",
      },
    });
    const slideWrap = screen.getByText("02 — Amazing Grace").closest(".slide-card-wrap")!;
    await fireEvent.dragOver(slideWrap);
    await fireEvent.dragLeave(slideWrap);
    await fireEvent.drop(slideWrap, {
      dataTransfer: {
        getData: (t: string) =>
          t === "application/x-openpresenter-media"
            ? JSON.stringify({ blake3_hash: "h-bg-1" })
            : "",
      },
    });
    await waitFor(() => expect(screen.getByText(/BACKGROUND: Aurora Wave/)).toBeTruthy());
    await fireEvent.click(screen.getByLabelText("Remove media cue from 02 — Amazing Grace"));
    await fireEvent.change(screen.getByLabelText("Mask Style"), {
      target: { value: "SoftVignette" },
    });
    await fireEvent.change(screen.getByLabelText("Look for Audience 1"), {
      target: { value: "look-2" },
    });
    await fireEvent.input(screen.getByLabelText("Stage Message"), {
      target: { value: "Nursery #14" },
    });
    await fireEvent.click(screen.getByText("Send Stage Msg"));
    await waitFor(() => expect(currentState.stage_message).toBe("Nursery #14"));

    // 5. Switch to Zero-Drift Slide Editor and mutate text, font size, stroke, alignment, position
    await fireEvent.click(screen.getByText("Slide Editor"));
    await waitFor(() => expect(screen.getByTestId("slide-editor")).toBeTruthy());
    await fireEvent.input(screen.getByLabelText("Lyric Text"), {
      target: { value: "New Edited Lyric" },
    });
    await fireEvent.input(screen.getByLabelText("Font Size"), { target: { value: "96" } });
    await fireEvent.input(screen.getByLabelText("Stroke Width"), { target: { value: "6" } });
    await fireEvent.click(screen.getByText("Left"));
    await fireEvent.click(screen.getByText("X →"));
    await fireEvent.click(screen.getByText("Y ↓"));

    // 5a. Switch to Screens configuration tab
    await fireEvent.click(screen.getByText("Screens"));
    await waitFor(() => expect(screen.getByTestId("screens-panel")).toBeTruthy());

    // 5a2. Switch to Looks & Templates, Scripture, and Quick Lyrics tabs + hotkeys/Freeze/Undo/Redo
    await fireEvent.click(screen.getByText("Looks & Templates"));
    await waitFor(() => expect(screen.getByTestId("looks-templates-panel")).toBeTruthy());

    await fireEvent.click(screen.getByText("Scripture"));
    await waitFor(() => expect(screen.getByTestId("scripture-panel")).toBeTruthy());

    await fireEvent.click(screen.getByText("Quick Lyrics"));
    await waitFor(() => expect(screen.getByTestId("quick-lyrics-panel")).toBeTruthy());

    await fireEvent.click(screen.getByText("Overlays & Msgs"));
    await waitFor(() => expect(screen.getByTestId("overlays-messages-panel")).toBeTruthy());

    await fireEvent.click(screen.getByText("Freeze"));
    await fireEvent.click(screen.getByText("Undo"));
    await fireEvent.click(screen.getByText("Redo"));
    await fireEvent.click(screen.getByText("Manage Looks"));
    await waitFor(() => expect(screen.getByTestId("looks-templates-panel")).toBeTruthy());
    await fireEvent.click(screen.getByText("Configure"));
    await waitFor(() => expect(screen.getByTestId("screens-panel")).toBeTruthy());
    await fireEvent.click(screen.getAllByText("Show")[0]);
    await waitFor(() => expect(screen.getByLabelText("Media Fit Mode")).toBeTruthy());
    await fireEvent.change(screen.getByLabelText("Media Fit Mode"), { target: { value: "Contain" } });
    await fireEvent.click(screen.getByText("Stop Audio"));
    await fireEvent.keyDown(window, { key: "ArrowRight" });
    await fireEvent.keyDown(window, { key: "ArrowLeft" });
    await fireEvent.keyDown(window, { key: "f" });
    await fireEvent.keyDown(window, { key: "b" });
    await fireEvent.keyDown(window, { key: "v" });
    await fireEvent.keyDown(window, { key: "c" });
    await fireEvent.keyDown(window, { key: "g" });
    await fireEvent.keyDown(window, { key: "F1" });
    await fireEvent.keyDown(window, { key: "F2" });
    await fireEvent.keyDown(window, { key: "z", ctrlKey: true });
    await fireEvent.keyDown(window, { key: "y", ctrlKey: true });

    // 5b. Switch to Diagnostics tab and verify global window error crash reporting hook
    await fireEvent.click(screen.getByText(/Diagnostics/));
    await waitFor(() => expect(screen.getByTestId("diagnostics-panel")).toBeTruthy());
    window.dispatchEvent(new ErrorEvent("error", { message: "Simulated UI error" }));

    // 6. Clean engine shutdown and full app Shutdown All
    await fireEvent.click(screen.getByText("Stop Engine"));
    await waitFor(() =>
      expect(screen.getAllByText("Engine shut down cleanly").length).toBeGreaterThan(0),
    );
    await fireEvent.click(screen.getByText("Shutdown All"));
    await waitFor(() =>
      expect(screen.getAllByText("Shutting down all windows and engines...").length).toBeGreaterThan(0),
    );
  });

  it("displays explicit error alerts and recovers cleanly on IPC and library failures (failure paths)", async () => {
    let shouldFailConnect = true;
    const snap = makeFixtureSnapshot();

    mockIPC((cmd) => {
      if (cmd === "connect_engine") {
        if (shouldFailConnect) {
          throw new Error("sidecar pipe refused");
        }
        return snap;
      }
      if (cmd === "list_libraries" || cmd === "list_playlists" || cmd === "list_media_bins" || cmd === "list_media_assets") {
        return [];
      }
      if (cmd === "search_library_songs") {
        if (!shouldFailConnect) {
          throw new Error("sqlite busy");
        }
        return [];
      }
      if (cmd === "set_blackout") {
        throw new Error("engine socket disconnected");
      }
      if (cmd === "update_slide_element") {
        throw new Error("invalid element bounds");
      }
      if (cmd === "poll_preview_frame") {
        return new ArrayBuffer(0);
      }
      return null;
    });

    render(App);

    // 1. Initial connection failure surfaces alert banner and fallback reconnect button
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("sidecar pipe refused"));
    expect(screen.getByText("Launch / Attach Engine")).toBeTruthy();

    // 2. Recover via Reconnect button and verify search failure surfaces cleanly
    shouldFailConnect = false;
    await fireEvent.click(screen.getByText("Launch / Attach Engine"));
    await fireEvent.input(screen.getByLabelText("Search songs"), { target: { value: "grace" } });
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("sqlite busy"));

    // 3. Blackout IPC failure surfaces alert banner without crashing UI
    await fireEvent.click(screen.getByText("BLACKOUT"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("engine socket disconnected"),
    );

    // 4. Slide Editor failure path surfaces inline error alert
    await fireEvent.click(screen.getByText("Slide Editor"));
    await fireEvent.input(screen.getByLabelText("Lyric Text"), {
      target: { value: "Trigger failure" },
    });
    await waitFor(() =>
      expect(screen.getAllByRole("alert").some((el) => el.textContent?.includes("invalid element bounds"))).toBe(true),
    );
  });
});
