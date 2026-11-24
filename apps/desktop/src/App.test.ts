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
        case "search_library_songs":
          return sampleSongs;
        case "start_preview_stream":
          return null;
        case "trigger_slide": {
          const found = currentState.deck.find((s) => s.id === payload.slideId) ?? null;
          currentState = { ...currentState, active_slide: found, blackout: false };
          return structuredClone(currentState);
        }
        case "set_blackout":
          currentState = { ...currentState, blackout: Boolean(payload.enabled) };
          return structuredClone(currentState);
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
        case "shutdown_engine":
          return null;
        default:
          return null;
      }
    });

    render(App);

    // 1. Engine attaches and displays GPU + offline library song
    await waitFor(() => expect(screen.getByText(/Test GPU D3D12/)).toBeTruthy());
    expect(screen.getByText("Amazing Grace")).toBeTruthy();

    // 2. Trigger slide 2
    await fireEvent.click(screen.getByText("02 — Amazing Grace"));
    await waitFor(() => expect(currentState.active_slide?.label).toBe("02 — Amazing Grace"));

    // 3. Toggle blackout ON and OFF
    await fireEvent.click(screen.getByText("BLACKOUT"));
    await waitFor(() => expect(screen.getByText("BLACKOUT ACTIVE")).toBeTruthy());
    await fireEvent.click(screen.getByText("BLACKOUT ACTIVE"));
    await waitFor(() => expect(screen.getByText("BLACKOUT")).toBeTruthy());

    // 4. Toggle overlay, clear slide, clear all, change mask, change look, send stage message
    await fireEvent.click(screen.getByText(/Overlay Bug:/));
    await fireEvent.click(screen.getByText("Clear Slide"));
    await fireEvent.click(screen.getByText("Clear All"));
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

    // 6. Clean engine shutdown
    await fireEvent.click(screen.getByText("Stop Engine"));
    await waitFor(() =>
      expect(screen.getAllByText("Engine shut down cleanly").length).toBeGreaterThan(0),
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
      if (cmd === "search_library_songs") {
        throw new Error("sqlite busy");
      }
      if (cmd === "set_blackout") {
        throw new Error("engine socket disconnected");
      }
      if (cmd === "update_slide_element") {
        throw new Error("invalid element bounds");
      }
      if (cmd === "start_preview_stream") {
        return null;
      }
      return null;
    });

    render(App);

    // 1. Initial connection failure surfaces alert banner and fallback reconnect button
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("sidecar pipe refused"));
    expect(screen.getByText("Launch / Attach Engine")).toBeTruthy();

    // 2. Recover via Reconnect button -> library search failure surfaces clearly
    shouldFailConnect = false;
    await fireEvent.click(screen.getByText("Launch / Attach Engine"));
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
