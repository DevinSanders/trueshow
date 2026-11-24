import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import OverlaysMessagesPanel from "./OverlaysMessagesPanel.svelte";
import type { OverlayItem, SceneState } from "./types";

function makeScene(): SceneState {
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
    overlays: {
      "ov-1": {
        id: "ov-1",
        name: "Church Bug",
        text: "LIVE",
        bounds: { x: 1400, y: 40, width: 400, height: 80 },
        style: {
          font_family: "sans-serif",
          font_size: 28,
          bold: true,
          italic: false,
          line_height: 1.2,
          color: { r: 1, g: 1, b: 1, a: 1 },
          stroke_color: { r: 0, g: 0, b: 0, a: 1 },
          stroke_width: 1,
          shadow_color: { r: 0, g: 0, b: 0, a: 0.5 },
          shadow_offset_x: 0,
          shadow_offset_y: 2,
          shadow_blur: 4,
          backdrop_color: { r: 0.7, g: 0.1, b: 0.1, a: 0.9 },
          backdrop_radius: 10,
          backdrop_padding: 12,
          h_align: "Center",
          v_align: "Middle",
          auto_shrink: true,
        },
        locked: true,
      },
    },
    active_overlay_ids: [],
    variables: { TeamA: "5" },
    outputs: [],
    deck: [],
  };
}

describe("OverlaysMessagesPanel (Overlays, Messages, Dynamic Variables)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("toggles/creates/deletes overlays, triggers tokenized messages, and updates dynamic variables", async () => {
    let state = makeScene();
    const onSceneChange = vi.fn((next: SceneState) => {
      state = next;
    });

    mockIPC((cmd, args) => {
      const p = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "list_message_templates":
          return [
            {
              id: "msg-nursery",
              name: "Nursery Parent Paging",
              template_text: "Parent #{code}, check Room {room}",
              show_on_audience: true,
              show_on_stage: true,
            },
          ];
        case "toggle_overlay_item":
          return { ...state, active_overlay_ids: p.active ? [String(p.overlayId)] : [] };
        case "upsert_overlay": {
          const ov = p.overlay as OverlayItem;
          return { ...state, overlays: { ...(state.overlays ?? {}), [ov.id]: ov } };
        }
        case "delete_overlay":
          return state;
        case "trigger_message":
        case "clear_message":
          return state;
        case "set_variable": {
          const nextVars = { ...(state.variables ?? {}) };
          if (p.value === null) delete nextVars[String(p.key)];
          else nextVars[String(p.key)] = String(p.value);
          return { ...state, variables: nextVars };
        }
        default:
          return null;
      }
    });

    render(OverlaysMessagesPanel, { props: { scene: state, onSceneChange } });
    await waitFor(() => {
      expect(screen.getByText("Nursery Parent Paging")).toBeTruthy();
    });

    // Toggle, save (empty validation + valid), and delete overlay
    await fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await fireEvent.input(screen.getByLabelText("Overlay Name"), { target: { value: "  " } });
    await fireEvent.click(screen.getByRole("button", { name: "Save Overlay" }));
    expect(screen.getByRole("alert").textContent).toContain("required");
    await fireEvent.input(screen.getByLabelText("Overlay Name"), { target: { value: "Welcome Bug" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save Overlay" }));
    await fireEvent.click(screen.getByRole("button", { name: "Del" }));

    // Trigger tokenized message & clear
    await fireEvent.input(screen.getByLabelText("Token code"), { target: { value: "99" } });
    await fireEvent.click(screen.getByRole("button", { name: "Show Message Live" }));
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("Layer 6");
    });
    await fireEvent.click(screen.getByRole("button", { name: "Clear Message" }));

    // Variables set, +1, -1, remove
    await fireEvent.click(screen.getByRole("button", { name: "Set Variable" }));
    await fireEvent.click(screen.getByRole("button", { name: "+1" }));
    await fireEvent.click(screen.getByRole("button", { name: "-1" }));
    await fireEvent.click(screen.getByRole("button", { name: "×" }));
    expect(onSceneChange).toHaveBeenCalled();
  });
});
