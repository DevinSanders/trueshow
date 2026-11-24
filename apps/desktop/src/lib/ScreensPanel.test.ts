import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import ScreensPanel from "./ScreensPanel.svelte";
import type { EngineSnapshot } from "./types";

function makeSnapshot(): EngineSnapshot {
  return {
    protocol_version: 1,
    engine_pid: 2026,
    gpu_adapter_name: "RTX 5080",
    gpu_backend: "Vulkan",
    displays: [
      {
        index: 0,
        name: "Primary Studio",
        width: 2560,
        height: 1440,
        scale_factor: 1.0,
        refresh_millihertz: 60000,
        position_x: 0,
        position_y: 0,
      },
      {
        index: 1,
        name: "LED Processor 4K",
        width: 3840,
        height: 2160,
        scale_factor: 1.0,
        refresh_millihertz: 60000,
        position_x: 2560,
        position_y: 0,
      },
    ],
    preview_buses: [],
    telemetry: {
      fps: 60,
      avg_frame_ms: 16.5,
      p99_frame_ms: 16.8,
      total_frames: 60,
      dropped_frames: 0,
      outputs: [],
    },
    state: {
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
      looks: {
        "look-aud": {
          id: "look-aud",
          name: "Audience Full",
          enabled_layers: ["MediaBackground", "Slide"],
          lower_third_mode: false,
          transparent_background: false,
          slide_blend_mode: "Normal",
        },
        "look-stg": {
          id: "look-stg",
          name: "Stage Confidence",
          enabled_layers: ["Slide"],
          lower_third_mode: false,
          transparent_background: false,
          slide_blend_mode: "Normal",
        },
      },
      outputs: [
        {
          id: "out-1",
          name: "Audience 1",
          kind: "Audience",
          monitor_index: 0,
          width: 1280,
          height: 720,
          fullscreen: false,
          placement: "Windowed",
          look_id: "look-aud",
        },
      ],
      deck: [],
    },
  };
}

describe("ScreensPanel (Multi-Screen, Fullscreen & Split-Monitor Regions)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("creates new Stage/Audience screens, splits a physical monitor into regions, edits, and removes screens", async () => {
    let current = makeSnapshot();
    const onSnapshotChange = vi.fn((next: EngineSnapshot) => {
      current = next;
    });

    mockIPC((cmd, args) => {
      const payload = (args ?? {}) as Record<string, unknown>;
      if (cmd === "upsert_output_screen") {
        const s = payload.screen as {
          id: string | null;
          name: string;
          kind: "Audience" | "Stage";
          monitor_index: number | null;
          width: number;
          height: number;
          placement_mode: string;
          region_x: number | null;
          region_y: number | null;
          region_width: number | null;
          region_height: number | null;
        };
        const placement =
          s.placement_mode === "fullscreen"
            ? ("Fullscreen" as const)
            : s.placement_mode === "region"
              ? {
                  Region: {
                    x: s.region_x ?? 0,
                    y: s.region_y ?? 0,
                    width: s.region_width ?? s.width,
                    height: s.region_height ?? s.height,
                  },
                }
              : ("Windowed" as const);
        const out = {
          id: s.id ?? `out-${current.state.outputs.length + 1}`,
          name: s.name,
          kind: s.kind,
          monitor_index: s.monitor_index,
          width: s.width,
          height: s.height,
          fullscreen: s.placement_mode === "fullscreen",
          placement,
          look_id: "look-aud",
        };
        const existingIdx = current.state.outputs.findIndex((o) => o.id === out.id);
        const nextOutputs =
          existingIdx >= 0
            ? current.state.outputs.map((o, i) => (i === existingIdx ? out : o))
            : [...current.state.outputs, out];
        return { ...current, state: { ...current.state, outputs: nextOutputs } };
      }
      if (cmd === "split_monitor_into_screens") {
        const monIdx = Number(payload.monitorIndex);
        return {
          ...current,
          state: {
            ...current.state,
            outputs: [
              ...current.state.outputs,
              {
                id: "split-left",
                name: `Display ${monIdx + 1} — Region 1`,
                kind: "Audience" as const,
                monitor_index: monIdx,
                width: 1920,
                height: 2160,
                fullscreen: false,
                placement: { Region: { x: 0, y: 0, width: 1920, height: 2160 } },
                look_id: "look-aud",
              },
              {
                id: "split-right",
                name: `Display ${monIdx + 1} — Region 2`,
                kind: "Stage" as const,
                monitor_index: monIdx,
                width: 1920,
                height: 2160,
                fullscreen: false,
                placement: { Region: { x: 1920, y: 0, width: 1920, height: 2160 } },
                look_id: "look-stg",
              },
            ],
          },
        };
      }
      if (cmd === "remove_output_screen") {
        return {
          ...current,
          state: {
            ...current.state,
            outputs: current.state.outputs.filter((o) => o.id !== payload.outputId),
          },
        };
      }
      return null;
    });

    const { rerender } = render(ScreensPanel, {
      props: { snapshot: current, onSnapshotChange },
    });

    expect(screen.getAllByText(/Primary Studio/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/LED Processor 4K/).length).toBeGreaterThan(0);

    // 1. Create a new Stage display placed in a custom monitor sub-region via Quick Area Preset
    await fireEvent.click(screen.getByText("+ New Stage"));
    await fireEvent.change(screen.getByLabelText("Physical Monitor"), { target: { value: "1" } });
    await fireEvent.change(screen.getByLabelText("Placement Mode"), {
      target: { value: "region" },
    });
    await fireEvent.click(screen.getByText("Right ½"));
    await fireEvent.click(screen.getByText("Create Stage Screen"));

    await waitFor(() => expect(onSnapshotChange).toHaveBeenCalledTimes(1));
    await rerender({ snapshot: current, onSnapshotChange });
    expect(screen.getByText(/REGION 1920×2160 @ \(1920,0\)/)).toBeTruthy();

    // 2. Split physical Display 2 into 2x1 independent regions
    const splitButtons = screen.getAllByText("Split 2×1 (L/R)");
    await fireEvent.click(splitButtons[1]);
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("Divided Display 2"),
    );
    await rerender({ snapshot: current, onSnapshotChange });

    // 3. Assign Display 1 Full Screen
    const fsButtons = screen.getAllByText("Full Screen");
    await fireEvent.click(fsButtons[0]);
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("Assigned Display 1 as Fullscreen"),
    );
    await rerender({ snapshot: current, onSnapshotChange });

    // 4. Edit existing screen and Remove a screen
    const editButtons = screen.getAllByText("Edit");
    await fireEvent.click(editButtons[0]);
    await fireEvent.click(screen.getByText("Apply Changes Live"));
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Updated screen"));

    const removeButtons = screen.getAllByText("Remove");
    await fireEvent.click(removeButtons[0]);
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Removed screen"));
  });

  it("validates empty screen names and surfaces IPC failures cleanly", async () => {
    const snap = makeSnapshot();
    const onSnapshotChange = vi.fn();

    mockIPC((cmd) => {
      if (cmd === "upsert_output_screen") throw new Error("monitor disconnected");
      if (cmd === "split_monitor_into_screens") throw new Error("invalid split dimensions");
      if (cmd === "remove_output_screen") throw new Error("engine offline");
      return null;
    });

    render(ScreensPanel, { props: { snapshot: snap, onSnapshotChange } });

    // Validation failure: empty screen name
    await fireEvent.input(screen.getByLabelText("Screen Name"), { target: { value: "   " } });
    await fireEvent.click(screen.getByText("Create Audience Screen"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("Please enter a screen name"),
    );

    // IPC upsert failure
    await fireEvent.input(screen.getByLabelText("Screen Name"), { target: { value: "Lobby TV" } });
    await fireEvent.click(screen.getByText("Create Audience Screen"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("monitor disconnected"),
    );

    // IPC split failure
    await fireEvent.click(screen.getAllByText("Split 2×2 (4×)")[0]);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("invalid split dimensions"),
    );

    // IPC remove failure
    await fireEvent.click(screen.getByText("Remove"));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("engine offline"));
  });
});
