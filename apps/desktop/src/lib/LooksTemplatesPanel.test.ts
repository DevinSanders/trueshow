import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import LooksTemplatesPanel from "./LooksTemplatesPanel.svelte";
import type { LookConfig, LookPresetRecord, SceneState, TemplateConfig } from "./types";

const BUILTIN_AUDIENCE = "00000000-0000-0000-0000-000000000001";
const BUILTIN_STAGE = "00000000-0000-0000-0000-000000000002";
const BUILTIN_LT = "00000000-0000-0000-0000-000000000003";

const TPL_CENTER = "00000000-0000-0000-0000-000000000001";
const TPL_LT = "00000000-0000-0000-0000-000000000002";

function makeScene(): SceneState {
  const slide1 = {
    id: "00000000-0000-0000-0000-0000000003e9",
    label: "01 — Welcome",
    group: "Pre-Service",
    headline: "WELCOME",
    body: "Gathering",
    chords_line: null,
    ccli_number: null,
    media_path: null,
    primary_color: { r: 0.1, g: 0.2, b: 0.3, a: 1 },
    secondary_color: { r: 0.2, g: 0.3, b: 0.5, a: 1 },
    accent_color: { r: 1, g: 0.8, b: 0.3, a: 1 },
    motion_style: "AuroraWave",
    transition: "Cut" as const,
    elements: [],
  };

  const baseStyle = {
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
  };

  return {
    revision: 1,
    blackout: false,
    active_slide: slide1,
    next_slide_preview: null,
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
      [BUILTIN_AUDIENCE]: {
        id: BUILTIN_AUDIENCE,
        name: "Audience Full",
        description: "Full 7-layer output",
        enabled_layers: [
          "VideoInput",
          "MediaBackground",
          "Slide",
          "Announcements",
          "PropsAndOverlays",
          "Masks",
          "MessagesAndTimers",
        ],
        lower_third_mode: false,
        transparent_background: false,
        slide_blend_mode: "Normal",
        template_id: null,
        mask_override: null,
      },
      [BUILTIN_STAGE]: {
        id: BUILTIN_STAGE,
        name: "Stage Confidence",
        description: "Stage confidence output",
        enabled_layers: ["Slide", "MessagesAndTimers"],
        lower_third_mode: false,
        transparent_background: false,
        slide_blend_mode: "Normal",
        template_id: null,
        mask_override: null,
      },
      [BUILTIN_LT]: {
        id: BUILTIN_LT,
        name: "Broadcast Lower Third",
        description: "Alpha lower third",
        enabled_layers: ["Slide", "PropsAndOverlays"],
        lower_third_mode: true,
        transparent_background: true,
        slide_blend_mode: "Normal",
        template_id: TPL_LT,
        mask_override: null,
      },
    },
    templates: {
      [TPL_CENTER]: {
        id: TPL_CENTER,
        name: "Sanctuary Full Center",
        category: "Lyrics",
        text_bounds: { x: 160, y: 230, width: 1600, height: 500 },
        subtitle_bounds: { x: 260, y: 770, width: 1400, height: 140 },
        text_style: baseStyle,
        subtitle_style: { ...baseStyle, font_size: 46, bold: false },
        accent_bar_color: null,
        default_mask: "None",
      },
      [TPL_LT]: {
        id: TPL_LT,
        name: "Broadcast Lower Third (Bar)",
        category: "Lower Third",
        text_bounds: { x: 120, y: 790, width: 1680, height: 200 },
        subtitle_bounds: null,
        text_style: {
          ...baseStyle,
          font_size: 54,
          backdrop_color: { r: 0.05, g: 0.08, b: 0.14, a: 0.88 },
        },
        subtitle_style: null,
        accent_bar_color: { r: 0.22, g: 0.62, b: 1, a: 0.96 },
        default_mask: "LowerThirdBand",
      },
    },
    outputs: [
      {
        id: "00000000-0000-0000-0000-000000000065",
        name: "Audience 1",
        kind: "Audience",
        monitor_index: 0,
        width: 1920,
        height: 1080,
        fullscreen: true,
        look_id: BUILTIN_AUDIENCE,
      },
      {
        id: "00000000-0000-0000-0000-000000000066",
        name: "Stage 1",
        kind: "Stage",
        monitor_index: 1,
        width: 1280,
        height: 720,
        fullscreen: false,
        look_id: BUILTIN_STAGE,
      },
    ],
    deck: [slide1],
  };
}

describe("LooksTemplatesPanel (Looks, Screen Routing, Presets, and Templates Designer)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("manages custom Looks, 7-layer toggles, screen routing, and presets (happy + validation paths)", async () => {
    let state = makeScene();
    let presets: LookPresetRecord[] = [
      {
        id: "p-1",
        name: "Worship Full",
        assignments: {
          "00000000-0000-0000-0000-000000000065": BUILTIN_AUDIENCE,
        },
        updated_at: "2026-11-24T00:00:00Z",
      },
    ];
    const onSceneChange = vi.fn((next: SceneState) => {
      state = next;
    });

    mockIPC((cmd, args) => {
      const p = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "list_look_presets":
          return structuredClone(presets);
        case "save_look_preset": {
          const rec: LookPresetRecord = {
            id: "p-new",
            name: String(p.name),
            assignments: (p.assignments as Record<string, string>) ?? {},
            updated_at: "2026-11-24T00:00:00Z",
          };
          presets = [...presets, rec];
          return structuredClone(presets);
        }
        case "apply_look_preset":
          return structuredClone(state);
        case "delete_look_preset":
          presets = presets.filter((x) => x.id !== p.presetId);
          return structuredClone(presets);
        case "set_output_look": {
          const next = structuredClone(state);
          const out = next.outputs.find((o) => o.id === p.outputId);
          if (out) out.look_id = String(p.lookId);
          state = next;
          return structuredClone(state);
        }
        case "apply_look_routing": {
          const next = structuredClone(state);
          const map = (p.assignments as Record<string, string>) ?? {};
          for (const [oid, lid] of Object.entries(map)) {
            const out = next.outputs.find((o) => o.id === oid);
            if (out) out.look_id = lid;
          }
          state = next;
          return structuredClone(state);
        }
        case "upsert_look": {
          const lk = p.look as LookConfig;
          const next = structuredClone(state);
          next.looks[lk.id] = lk;
          state = next;
          return structuredClone(state);
        }
        case "delete_look": {
          const next = structuredClone(state);
          delete next.looks[String(p.lookId)];
          state = next;
          return structuredClone(state);
        }
        default:
          return null;
      }
    });

    const { rerender } = render(LooksTemplatesPanel, {
      props: { scene: state, onSceneChange },
    });

    await waitFor(() => {
      expect(screen.getByText("Worship Full")).toBeTruthy();
    });

    // Save preset validation error (empty name) then valid save, recall, and delete
    await fireEvent.click(screen.getByRole("button", { name: "Save Routing Preset" }));
    expect(screen.getByRole("alert").textContent).toContain("preset name");

    await fireEvent.input(screen.getByLabelText("New Look Preset Name"), {
      target: { value: "Sermon Lower-Third" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save Routing Preset" }));
    await waitFor(() => {
      expect(screen.getByText("Sermon Lower-Third")).toBeTruthy();
    });

    await fireEvent.click(screen.getByRole("button", { name: "Sermon Lower-Third" }));
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("Recalled Look Preset");
    });

    await fireEvent.click(screen.getByLabelText("Delete preset Sermon Lower-Third"));
    await waitFor(() => {
      expect(screen.queryByText("Sermon Lower-Third")).toBeNull();
    });

    // Route Audience 1 directly in matrix
    const routeSelect = screen.getByLabelText("Route Look for Audience 1");
    await fireEvent.change(routeSelect, { target: { value: BUILTIN_LT } });
    await waitFor(() => {
      expect(onSceneChange).toHaveBeenCalled();
    });

    // Create a new custom Look, toggle a layer off/on, set template & mask overrides, and save
    await fireEvent.click(screen.getByRole("button", { name: "+ New Look" }));
    await fireEvent.input(screen.getByLabelText("Look Name"), { target: { value: "  " } });
    await fireEvent.click(screen.getByRole("button", { name: "Create Look" }));
    expect(screen.getByRole("alert").textContent).toContain("Look name");

    await fireEvent.input(screen.getByLabelText("Look Name"), {
      target: { value: "Lobby Clean Feed" },
    });
    await fireEvent.change(screen.getByLabelText("Slide Template Override"), {
      target: { value: TPL_LT },
    });
    await fireEvent.change(screen.getByLabelText("Mask Override"), {
      target: { value: "RoundedFrame" },
    });
    const l0Checkbox = screen.getByLabelText("L0 Video Input");
    await fireEvent.click(l0Checkbox);
    await fireEvent.click(l0Checkbox);
    await fireEvent.click(screen.getByRole("button", { name: "Create Look" }));

    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("Lobby Clean Feed");
    });
    await rerender({ scene: state, onSceneChange });

    // Bulk apply selected look to All Audience & All Stage
    await fireEvent.click(screen.getByRole("button", { name: "Apply → All Audience" }));
    await fireEvent.click(screen.getByRole("button", { name: "Apply → All Stage" }));

    // Duplicate & Delete a non-protected look
    const duplicateButtons = screen.getAllByRole("button", { name: "Duplicate" });
    await fireEvent.click(duplicateButtons[0]);
    const deleteButtons = screen.getAllByRole("button", { name: "Delete" });
    await fireEvent.click(deleteButtons[0]);
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("Deleted Look");
    });
  });

  it("designs, filters, saves, deletes, and applies Templates to active slide and entire deck", async () => {
    let state = makeScene();
    const onSceneChange = vi.fn((next: SceneState) => {
      state = next;
    });
    const onError = vi.fn();

    mockIPC((cmd, args) => {
      const p = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "list_look_presets":
          return [];
        case "upsert_template": {
          const tpl = p.template as TemplateConfig;
          const next = structuredClone(state);
          next.templates = { ...(next.templates ?? {}), [tpl.id]: tpl };
          state = next;
          return structuredClone(state);
        }
        case "delete_template": {
          const next = structuredClone(state);
          if (next.templates) delete next.templates[String(p.templateId)];
          state = next;
          return structuredClone(state);
        }
        case "apply_template_to_slide":
          return structuredClone(state);
        case "apply_template_to_deck":
          return structuredClone(state);
        default:
          return null;
      }
    });

    const { rerender } = render(LooksTemplatesPanel, {
      props: { scene: state, onSceneChange, onError },
    });

    // Switch to Templates Designer sub-tab
    await fireEvent.click(screen.getByRole("tab", { name: /Templates Designer/i }));
    await waitFor(() => {
      expect(screen.getByLabelText("Live Template 16:9 Preview")).toBeTruthy();
    });

    // Filter by category then back to All
    await fireEvent.click(screen.getAllByRole("button", { name: /^Lower Third$/ })[0]);
    await fireEvent.click(screen.getByRole("button", { name: /^All$/ }));

    // Apply existing template to active slide & entire deck
    const applySlideBtns = screen.getAllByRole("button", { name: "Apply Slide" });
    await fireEvent.click(applySlideBtns[0]);
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("active slide");
    });

    const applyDeckBtns = screen.getAllByRole("button", { name: "Apply Deck" });
    await fireEvent.click(applyDeckBtns[0]);
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("entire cue deck");
    });

    // Exercise all quick layout presets + create a new template (with empty-name validation check)
    await fireEvent.click(screen.getByRole("button", { name: "+ New Template" }));
    await fireEvent.click(screen.getAllByRole("button", { name: /^Lower Third$/ })[1]);
    await fireEvent.click(screen.getByRole("button", { name: "Upper Third" }));
    await fireEvent.click(screen.getByRole("button", { name: "Left Bold" }));
    await fireEvent.click(screen.getAllByRole("button", { name: /^Scripture$/ })[1]);
    await fireEvent.click(screen.getByRole("button", { name: "Center" }));

    await fireEvent.input(screen.getByLabelText("Template Name"), { target: { value: " " } });
    await fireEvent.click(screen.getByRole("button", { name: "Create Template" }));
    expect(screen.getByRole("alert").textContent).toContain("Template name");

    await fireEvent.input(screen.getByLabelText("Template Name"), {
      target: { value: "Sunday Announcement Card" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Create Template" }));
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("Sunday Announcement Card");
    });
    await rerender({ scene: state, onSceneChange, onError });

    // Copy & Delete a template
    const copyBtns = screen.getAllByRole("button", { name: "Copy" });
    await fireEvent.click(copyBtns[0]);
    const delBtns = screen.getAllByRole("button", { name: "Del" });
    await fireEvent.click(delBtns[0]);
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("Deleted Template");
    });
  });

  it("surfaces IPC errors gracefully when backend commands fail", async () => {
    const state = makeScene();
    const onSceneChange = vi.fn();
    const onError = vi.fn();

    mockIPC((cmd) => {
      if (cmd === "list_look_presets") throw new Error("db offline");
      throw new Error("engine disconnected");
    });

    render(LooksTemplatesPanel, {
      props: { scene: state, onSceneChange, onError },
    });

    await fireEvent.click(screen.getByRole("button", { name: "Save Look Live" }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain("Failed to save Look");
      expect(onError).toHaveBeenCalled();
    });
  });
});
