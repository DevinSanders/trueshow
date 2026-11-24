<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type {
    ColorRgba,
    LayerKind,
    LookConfig,
    LookPresetRecord,
    MaskStyle,
    RectF,
    SceneState,
    TemplateConfig,
    TextAlign,
    TextStyle,
    VerticalAlign,
  } from "./types";

  interface Props {
    scene: SceneState;
    onSceneChange: (next: SceneState) => void;
    onError?: (msg: string) => void;
  }

  let { scene, onSceneChange, onError }: Props = $props();

  const BUILTIN_AUDIENCE_LOOK_ID = "00000000-0000-0000-0000-000000000001";
  const BUILTIN_STAGE_LOOK_ID = "00000000-0000-0000-0000-000000000002";

  const ALL_LAYERS: { kind: LayerKind; code: string; label: string }[] = [
    { kind: "VideoInput", code: "L0", label: "Video Input" },
    { kind: "MediaBackground", code: "L1", label: "Media / Background" },
    { kind: "Slide", code: "L2", label: "Slide / Foreground" },
    { kind: "Announcements", code: "L3", label: "Announcements" },
    { kind: "PropsAndOverlays", code: "L4", label: "Props & Overlays" },
    { kind: "Masks", code: "L5", label: "Masks" },
    { kind: "MessagesAndTimers", code: "L6", label: "Messages & Timers" },
  ];

  const TEMPLATE_CATEGORIES = ["All", "Lyrics", "Lower Third", "Scripture", "Announcement", "Custom"];

  let subTab = $state<"looks" | "templates">("looks");
  let busy = $state(false);
  let notice = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  // Saved multi-screen routing presets
  let presets = $state<LookPresetRecord[]>([]);
  let newPresetName = $state("");

  // Look editor state
  let editingLookId = $state<string | null>(null);
  let lookName = $state("");
  let lookDescription = $state("");
  let lookLayers = $state<LayerKind[]>([
    "VideoInput",
    "MediaBackground",
    "Slide",
    "Announcements",
    "PropsAndOverlays",
    "Masks",
    "MessagesAndTimers",
  ]);
  let lookLowerThird = $state(false);
  let lookTransparentBg = $state(false);
  let lookBlendMode = $state("Normal");
  let lookTemplateId = $state<string>("");
  let lookMaskOverride = $state<string>("inherit");

  // Template editor state
  let templateCategoryFilter = $state("All");
  let editingTemplateId = $state<string | null>(null);
  let tplName = $state("");
  let tplCategory = $state("Lyrics");
  let tplBoundsX = $state(160);
  let tplBoundsY = $state(230);
  let tplBoundsW = $state(1600);
  let tplBoundsH = $state(500);
  let tplFontFamily = $state("sans-serif");
  let tplFontSize = $state(84);
  let tplBold = $state(true);
  let tplItalic = $state(false);
  let tplLineHeight = $state(1.22);
  let tplHAlign = $state<TextAlign>("Center");
  let tplVAlign = $state<VerticalAlign>("Middle");
  let tplAutoShrink = $state(true);
  let tplTextHex = $state("#ffffff");
  let tplStrokeHex = $state("#000000");
  let tplStrokeWidth = $state(4);
  let tplShadowBlur = $state(10);
  let tplShadowOffsetY = $state(6);
  let tplBackdropHex = $state("#0c1424");
  let tplBackdropAlpha = $state(0);
  let tplBackdropRadius = $state(18);
  let tplBackdropPadding = $state(24);
  let tplHasAccentBar = $state(false);
  let tplAccentHex = $state("#389eff");
  let tplHasSubtitle = $state(true);
  let tplSubY = $state(770);
  let tplSubH = $state(140);
  let tplSubFontSize = $state(44);
  let tplSubHex = $state("#e5efff");
  let tplDefaultMask = $state<MaskStyle>("None");

  const looksList = $derived(
    Object.values(scene.looks ?? {}).sort((a, b) => a.name.localeCompare(b.name)),
  );
  const templatesList = $derived(
    Object.values(scene.templates ?? {}).sort((a, b) => a.name.localeCompare(b.name)),
  );
  const filteredTemplates = $derived(
    templateCategoryFilter === "All"
      ? templatesList
      : templatesList.filter((t) => t.category.toLowerCase() === templateCategoryFilter.toLowerCase()),
  );

  function newUuid(): string {
    if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
      return crypto.randomUUID();
    }
    return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
      const r = Math.floor(Math.random() * 16);
      const v = c === "x" ? r : (r & 0x3) | 0x8;
      return v.toString(16);
    });
  }

  function rgbaToHex(c: ColorRgba | null | undefined, fallback = "#ffffff"): string {
    if (!c) return fallback;
    const hex = (v: number) =>
      Math.max(0, Math.min(255, Math.round(v * 255)))
        .toString(16)
        .padStart(2, "0");
    return `#${hex(c.r)}${hex(c.g)}${hex(c.b)}`;
  }

  function hexToRgba(hex: string, alpha = 1): ColorRgba {
    const clean = hex.replace("#", "").trim();
    if (clean.length !== 6) {
      return { r: 1, g: 1, b: 1, a: alpha };
    }
    const r = parseInt(clean.slice(0, 2), 16) / 255;
    const g = parseInt(clean.slice(2, 4), 16) / 255;
    const b = parseInt(clean.slice(4, 6), 16) / 255;
    return {
      r: Number.isFinite(r) ? r : 1,
      g: Number.isFinite(g) ? g : 1,
      b: Number.isFinite(b) ? b : 1,
      a: Math.max(0, Math.min(1, alpha)),
    };
  }

  function cssRgba(c: ColorRgba): string {
    return `rgba(${Math.round(c.r * 255)}, ${Math.round(c.g * 255)}, ${Math.round(c.b * 255)}, ${c.a.toFixed(2)})`;
  }

  function reportError(err: unknown, prefix: string) {
    const msg = `${prefix}: ${String(err)}`;
    errorMsg = msg;
    onError?.(msg);
  }

  async function loadPresets() {
    try {
      const loaded = await invoke<LookPresetRecord[]>("list_look_presets");
      presets = Array.isArray(loaded) ? loaded : [];
    } catch {
      presets = [];
    }
  }

  function selectLookForEdit(look: LookConfig) {
    editingLookId = look.id;
    lookName = look.name;
    lookDescription = look.description ?? "";
    lookLayers = [...look.enabled_layers];
    lookLowerThird = look.lower_third_mode;
    lookTransparentBg = look.transparent_background;
    lookBlendMode = look.slide_blend_mode || "Normal";
    lookTemplateId = look.template_id ?? "";
    lookMaskOverride = look.mask_override ?? "inherit";
    notice = null;
    errorMsg = null;
  }

  function startNewLook(base?: LookConfig) {
    editingLookId = null;
    lookName = base ? `${base.name} (Copy)` : `Custom Look ${looksList.length + 1}`;
    lookDescription = base?.description ?? "Custom per-screen layer & template routing";
    lookLayers = base ? [...base.enabled_layers] : ALL_LAYERS.map((l) => l.kind);
    lookLowerThird = base?.lower_third_mode ?? false;
    lookTransparentBg = base?.transparent_background ?? false;
    lookBlendMode = base?.slide_blend_mode ?? "Normal";
    lookTemplateId = base?.template_id ?? "";
    lookMaskOverride = base?.mask_override ?? "inherit";
    notice = null;
    errorMsg = null;
  }

  function toggleLookLayer(kind: LayerKind) {
    if (lookLayers.includes(kind)) {
      lookLayers = lookLayers.filter((l) => l !== kind);
    } else {
      lookLayers = [...lookLayers, kind];
    }
  }

  async function saveLook() {
    errorMsg = null;
    notice = null;
    const trimmed = lookName.trim();
    if (!trimmed) {
      errorMsg = "Please enter a Look name.";
      return;
    }
    busy = true;
    try {
      const look: LookConfig = {
        id: editingLookId ?? newUuid(),
        name: trimmed,
        description: lookDescription.trim(),
        enabled_layers: [...lookLayers],
        lower_third_mode: lookLowerThird,
        transparent_background: lookTransparentBg,
        slide_blend_mode: lookBlendMode,
        template_id: lookTemplateId || null,
        mask_override: lookMaskOverride === "inherit" ? null : (lookMaskOverride as MaskStyle),
      };
      const next = await invoke<SceneState>("upsert_look", { look });
      onSceneChange(next);
      editingLookId = look.id;
      notice = `Saved Look "${trimmed}" live.`;
    } catch (err) {
      reportError(err, "Failed to save Look");
    } finally {
      busy = false;
    }
  }

  async function removeLook(look: LookConfig) {
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const next = await invoke<SceneState>("delete_look", { lookId: look.id });
      onSceneChange(next);
      if (editingLookId === look.id) {
        const first = Object.values(next.looks)[0];
        if (first) selectLookForEdit(first);
      }
      notice = `Deleted Look "${look.name}".`;
    } catch (err) {
      reportError(err, "Failed to delete Look");
    } finally {
      busy = false;
    }
  }

  async function assignLookToOutput(outputId: string, lookId: string) {
    errorMsg = null;
    notice = null;
    try {
      const next = await invoke<SceneState>("set_output_look", { outputId, lookId });
      onSceneChange(next);
    } catch (err) {
      reportError(err, "Failed to assign Look");
    }
  }

  async function applyLookToKind(kind: "Audience" | "Stage") {
    if (!editingLookId) return;
    errorMsg = null;
    notice = null;
    const assignments: Record<string, string> = {};
    for (const out of scene.outputs) {
      if (out.kind === kind) {
        assignments[out.id] = editingLookId;
      }
    }
    if (Object.keys(assignments).length === 0) {
      errorMsg = `No ${kind} outputs currently configured.`;
      return;
    }
    busy = true;
    try {
      const next = await invoke<SceneState>("apply_look_routing", { assignments });
      onSceneChange(next);
      notice = `Applied "${lookName}" to all ${kind} screens.`;
    } catch (err) {
      reportError(err, "Failed to route Look");
    } finally {
      busy = false;
    }
  }

  async function saveCurrentPreset() {
    errorMsg = null;
    notice = null;
    const trimmed = newPresetName.trim();
    if (!trimmed) {
      errorMsg = "Enter a preset name (e.g. Worship Mode or Sermon Lower-Third).";
      return;
    }
    const assignments: Record<string, string> = {};
    for (const out of scene.outputs) {
      assignments[out.id] = out.look_id;
    }
    busy = true;
    try {
      presets = await invoke<LookPresetRecord[]>("save_look_preset", {
        id: null,
        name: trimmed,
        assignments,
      });
      newPresetName = "";
      notice = `Saved Look Preset "${trimmed}".`;
    } catch (err) {
      reportError(err, "Failed to save preset");
    } finally {
      busy = false;
    }
  }

  async function recallPreset(preset: LookPresetRecord) {
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const next = await invoke<SceneState>("apply_look_preset", { presetId: preset.id });
      onSceneChange(next);
      notice = `Recalled Look Preset "${preset.name}" across screens.`;
    } catch (err) {
      reportError(err, "Failed to recall preset");
    } finally {
      busy = false;
    }
  }

  async function removePreset(preset: LookPresetRecord) {
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      presets = await invoke<LookPresetRecord[]>("delete_look_preset", { presetId: preset.id });
      notice = `Deleted preset "${preset.name}".`;
    } catch (err) {
      reportError(err, "Failed to delete preset");
    } finally {
      busy = false;
    }
  }

  // ---- Templates Designer ----

  function selectTemplateForEdit(tpl: TemplateConfig) {
    editingTemplateId = tpl.id;
    tplName = tpl.name;
    tplCategory = tpl.category || "Custom";
    tplBoundsX = Math.round(tpl.text_bounds.x);
    tplBoundsY = Math.round(tpl.text_bounds.y);
    tplBoundsW = Math.round(tpl.text_bounds.width);
    tplBoundsH = Math.round(tpl.text_bounds.height);
    tplFontFamily = tpl.text_style.font_family || "sans-serif";
    tplFontSize = Math.round(tpl.text_style.font_size);
    tplBold = tpl.text_style.bold;
    tplItalic = tpl.text_style.italic;
    tplLineHeight = tpl.text_style.line_height;
    tplHAlign = tpl.text_style.h_align;
    tplVAlign = tpl.text_style.v_align;
    tplAutoShrink = tpl.text_style.auto_shrink;
    tplTextHex = rgbaToHex(tpl.text_style.color, "#ffffff");
    tplStrokeHex = rgbaToHex(tpl.text_style.stroke_color, "#000000");
    tplStrokeWidth = tpl.text_style.stroke_width;
    tplShadowBlur = tpl.text_style.shadow_blur;
    tplShadowOffsetY = tpl.text_style.shadow_offset_y;
    tplBackdropHex = rgbaToHex(tpl.text_style.backdrop_color, "#0c1424");
    tplBackdropAlpha = Math.round((tpl.text_style.backdrop_color?.a ?? 0) * 100);
    tplBackdropRadius = tpl.text_style.backdrop_radius;
    tplBackdropPadding = tpl.text_style.backdrop_padding;
    tplHasAccentBar = Boolean(tpl.accent_bar_color);
    tplAccentHex = rgbaToHex(tpl.accent_bar_color, "#389eff");
    tplHasSubtitle = Boolean(tpl.subtitle_bounds);
    tplSubY = Math.round(tpl.subtitle_bounds?.y ?? 770);
    tplSubH = Math.round(tpl.subtitle_bounds?.height ?? 140);
    tplSubFontSize = Math.round(tpl.subtitle_style?.font_size ?? 44);
    tplSubHex = rgbaToHex(tpl.subtitle_style?.color, "#e5efff");
    tplDefaultMask = tpl.default_mask ?? "None";
    notice = null;
    errorMsg = null;
  }

  function startNewTemplate(base?: TemplateConfig) {
    if (base) {
      selectTemplateForEdit(base);
      editingTemplateId = null;
      tplName = `${base.name} (Copy)`;
      return;
    }
    editingTemplateId = null;
    tplName = `Custom Template ${templatesList.length + 1}`;
    tplCategory = "Custom";
    applyTemplateLayoutPreset("center");
    notice = null;
    errorMsg = null;
  }

  function applyTemplateLayoutPreset(
    preset: "center" | "lower-third" | "upper-third" | "left-col" | "scripture",
  ) {
    if (preset === "lower-third") {
      tplBoundsX = 120;
      tplBoundsY = 790;
      tplBoundsW = 1680;
      tplBoundsH = 200;
      tplFontSize = 54;
      tplHAlign = "Center";
      tplVAlign = "Middle";
      tplBackdropAlpha = 88;
      tplHasAccentBar = true;
      tplHasSubtitle = false;
    } else if (preset === "upper-third") {
      tplBoundsX = 120;
      tplBoundsY = 80;
      tplBoundsW = 1680;
      tplBoundsH = 220;
      tplFontSize = 60;
      tplHAlign = "Center";
      tplVAlign = "Middle";
      tplBackdropAlpha = 72;
      tplHasAccentBar = false;
      tplHasSubtitle = false;
    } else if (preset === "left-col") {
      tplBoundsX = 120;
      tplBoundsY = 180;
      tplBoundsW = 1160;
      tplBoundsH = 620;
      tplFontSize = 86;
      tplHAlign = "Left";
      tplVAlign = "Middle";
      tplBackdropAlpha = 0;
      tplHasAccentBar = false;
      tplHasSubtitle = true;
      tplSubY = 830;
      tplSubH = 100;
    } else if (preset === "scripture") {
      tplBoundsX = 180;
      tplBoundsY = 220;
      tplBoundsW = 1560;
      tplBoundsH = 520;
      tplFontFamily = "serif";
      tplFontSize = 64;
      tplBold = false;
      tplHAlign = "Left";
      tplVAlign = "Middle";
      tplBackdropAlpha = 78;
      tplHasAccentBar = true;
      tplAccentHex = "#f2c759";
      tplHasSubtitle = true;
      tplSubY = 760;
      tplSubH = 110;
    } else {
      tplBoundsX = 160;
      tplBoundsY = 230;
      tplBoundsW = 1600;
      tplBoundsH = 500;
      tplFontFamily = "sans-serif";
      tplFontSize = 86;
      tplBold = true;
      tplHAlign = "Center";
      tplVAlign = "Middle";
      tplBackdropAlpha = 0;
      tplHasAccentBar = false;
      tplHasSubtitle = true;
      tplSubY = 770;
      tplSubH = 140;
    }
  }

  function buildTemplatePayload(id: string): TemplateConfig {
    const text_style: TextStyle = {
      font_family: tplFontFamily.trim() || "sans-serif",
      font_size: Number(tplFontSize),
      bold: tplBold,
      italic: tplItalic,
      line_height: Number(tplLineHeight),
      color: hexToRgba(tplTextHex, 1),
      stroke_color: hexToRgba(tplStrokeHex, 0.92),
      stroke_width: Number(tplStrokeWidth),
      shadow_color: { r: 0, g: 0, b: 0, a: tplShadowBlur > 0 ? 0.72 : 0 },
      shadow_offset_x: 0,
      shadow_offset_y: Number(tplShadowOffsetY),
      shadow_blur: Number(tplShadowBlur),
      backdrop_color: hexToRgba(tplBackdropHex, Number(tplBackdropAlpha) / 100),
      backdrop_radius: Number(tplBackdropRadius),
      backdrop_padding: Number(tplBackdropPadding),
      h_align: tplHAlign,
      v_align: tplVAlign,
      auto_shrink: tplAutoShrink,
    };

    const text_bounds: RectF = {
      x: Number(tplBoundsX),
      y: Number(tplBoundsY),
      width: Math.max(64, Number(tplBoundsW)),
      height: Math.max(48, Number(tplBoundsH)),
    };

    const subtitle_bounds: RectF | null = tplHasSubtitle
      ? {
          x: text_bounds.x,
          y: Number(tplSubY),
          width: text_bounds.width,
          height: Math.max(40, Number(tplSubH)),
        }
      : null;

    const subtitle_style: TextStyle | null = tplHasSubtitle
      ? {
          ...text_style,
          font_size: Number(tplSubFontSize),
          bold: false,
          color: hexToRgba(tplSubHex, 0.95),
          backdrop_color: { r: 0, g: 0, b: 0, a: 0 },
          stroke_width: Math.min(2.5, Number(tplStrokeWidth)),
        }
      : null;

    return {
      id,
      name: tplName.trim(),
      category: tplCategory,
      text_bounds,
      subtitle_bounds,
      text_style,
      subtitle_style,
      accent_bar_color: tplHasAccentBar ? hexToRgba(tplAccentHex, 0.96) : null,
      default_mask: tplDefaultMask,
    };
  }

  const livePreviewTemplate = $derived(buildTemplatePayload(editingTemplateId ?? "preview"));

  function previewBoxStyle(bounds: RectF, style: TextStyle, scaleFactor = 0.22): string {
    const left = (bounds.x / 1920) * 100;
    const top = (bounds.y / 1080) * 100;
    const width = (bounds.width / 1920) * 100;
    const height = (bounds.height / 1080) * 100;
    const justify =
      style.v_align === "Top" ? "flex-start" : style.v_align === "Bottom" ? "flex-end" : "center";
    const textAlign = style.h_align.toLowerCase();
    const bg = cssRgba(style.backdrop_color);
    const col = cssRgba(style.color);
    const fs = Math.max(8, Math.round(style.font_size * scaleFactor));
    return [
      `left:${left}%`,
      `top:${top}%`,
      `width:${width}%`,
      `height:${height}%`,
      `font-family:${style.font_family}`,
      `font-size:${fs}px`,
      `font-weight:${style.bold ? 700 : 400}`,
      `font-style:${style.italic ? "italic" : "normal"}`,
      `line-height:${style.line_height}`,
      `color:${col}`,
      `background:${bg}`,
      `border-radius:${Math.round(style.backdrop_radius * scaleFactor)}px`,
      `text-align:${textAlign}`,
      `justify-content:${justify}`,
    ].join(";");
  }

  function accentBarStyle(tpl: TemplateConfig): string {
    const left = (tpl.text_bounds.x / 1920) * 100;
    const top = (tpl.text_bounds.y / 1080) * 100;
    const height = (tpl.text_bounds.height / 1080) * 100;
    const color = tpl.accent_bar_color ? cssRgba(tpl.accent_bar_color) : "transparent";
    return `left:${left}%;top:${top}%;height:${height}%;background:${color};`;
  }

  async function saveTemplate() {
    errorMsg = null;
    notice = null;
    if (!tplName.trim()) {
      errorMsg = "Please enter a Template name.";
      return;
    }
    busy = true;
    try {
      const id = editingTemplateId ?? newUuid();
      const template = buildTemplatePayload(id);
      const next = await invoke<SceneState>("upsert_template", { template });
      onSceneChange(next);
      editingTemplateId = id;
      notice = `Saved Template "${template.name}" live.`;
    } catch (err) {
      reportError(err, "Failed to save Template");
    } finally {
      busy = false;
    }
  }

  async function removeTemplate(tpl: TemplateConfig) {
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const next = await invoke<SceneState>("delete_template", { templateId: tpl.id });
      onSceneChange(next);
      if (editingTemplateId === tpl.id) {
        const first = Object.values(next.templates ?? {})[0];
        if (first) selectTemplateForEdit(first);
      }
      notice = `Deleted Template "${tpl.name}".`;
    } catch (err) {
      reportError(err, "Failed to delete Template");
    } finally {
      busy = false;
    }
  }

  async function applyTemplateToActiveSlide(templateId: string, label: string) {
    const activeId = scene.active_slide?.id ?? scene.deck[0]?.id;
    if (!activeId) {
      errorMsg = "No active slide available.";
      return;
    }
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const next = await invoke<SceneState>("apply_template_to_slide", {
        slideId: activeId,
        templateId,
      });
      onSceneChange(next);
      notice = `Applied template "${label}" to active slide.`;
    } catch (err) {
      reportError(err, "Failed to apply template to slide");
    } finally {
      busy = false;
    }
  }

  async function applyTemplateToDeck(templateId: string, label: string) {
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const next = await invoke<SceneState>("apply_template_to_deck", { templateId });
      onSceneChange(next);
      notice = `Applied template "${label}" to entire cue deck (${next.deck.length} slides).`;
    } catch (err) {
      reportError(err, "Failed to apply template to deck");
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    void loadPresets();
    if (looksList.length > 0 && !editingLookId) {
      selectLookForEdit(looksList[0]);
    }
    if (templatesList.length > 0 && !editingTemplateId) {
      selectTemplateForEdit(templatesList[0]);
    }
  });
</script>

<section class="lt-panel" data-testid="looks-templates-panel">
  <header class="lt-head">
    <div>
      <h2>Looks & Templates Manager</h2>
      <p class="sub">
        Create custom per-screen Looks (layer visibility, alpha keying, template overrides) and reusable slide Templates.
      </p>
    </div>
    <div class="mode-switch" role="tablist" aria-label="Looks and Templates sections">
      <button
        role="tab"
        aria-selected={subTab === "looks"}
        class="mode-btn"
        class:active={subTab === "looks"}
        onclick={() => (subTab = "looks")}
      >
        Looks & Routing ({looksList.length})
      </button>
      <button
        role="tab"
        aria-selected={subTab === "templates"}
        class="mode-btn"
        class:active={subTab === "templates"}
        onclick={() => (subTab = "templates")}
      >
        Templates Designer ({templatesList.length})
      </button>
    </div>
  </header>

  {#if errorMsg}
    <div class="banner error" role="alert">{errorMsg}</div>
  {/if}
  {#if notice}
    <div class="banner ok" role="status">{notice}</div>
  {/if}

  {#if subTab === "looks"}
    <div class="card">
      <div class="card-head">
        <h3>Live Screen Routing Matrix & Presets</h3>
        <div class="preset-save">
          <input
            aria-label="New Look Preset Name"
            placeholder="Preset name (e.g. Sermon Lower-Third)..."
            bind:value={newPresetName}
          />
          <button class="btn sm primary" disabled={busy} onclick={() => void saveCurrentPreset()}>
            Save Routing Preset
          </button>
        </div>
      </div>

      {#if presets.length > 0}
        <div class="preset-chips">
          <span class="chip-label">Presets:</span>
          {#each presets as preset (preset.id)}
            <div class="preset-chip">
              <button
                class="preset-recall"
                disabled={busy}
                onclick={() => void recallPreset(preset)}
              >
                {preset.name}
              </button>
              <button
                class="preset-del"
                aria-label={`Delete preset ${preset.name}`}
                disabled={busy}
                onclick={() => void removePreset(preset)}
              >
                ×
              </button>
            </div>
          {/each}
        </div>
      {/if}

      <div class="routing-grid">
        {#each scene.outputs as out (out.id)}
          <div class="route-row">
            <div class="route-meta">
              <span class="kind-pill" class:stage={out.kind === "Stage"}>{out.kind}</span>
              <strong>{out.name}</strong>
            </div>
            <select
              aria-label={`Route Look for ${out.name}`}
              value={out.look_id}
              onchange={(e) => void assignLookToOutput(out.id, e.currentTarget.value)}
            >
              {#each looksList as lk (lk.id)}
                <option value={lk.id}>{lk.name}</option>
              {/each}
            </select>
          </div>
        {/each}
      </div>
    </div>

    <div class="two-col">
      <div class="card">
        <div class="card-head">
          <h3>Looks Library ({looksList.length})</h3>
          <button class="btn sm primary" onclick={() => startNewLook()}>+ New Look</button>
        </div>
        <ul class="item-list">
          {#each looksList as lk (lk.id)}
            {@const assignedCount = scene.outputs.filter((o) => o.look_id === lk.id).length}
            {@const tplNameBadge = lk.template_id
              ? scene.templates?.[lk.template_id]?.name ?? null
              : null}
            {@const isProtected =
              lk.id === BUILTIN_AUDIENCE_LOOK_ID || lk.id === BUILTIN_STAGE_LOOK_ID}
            <li class="item-row" class:selected={editingLookId === lk.id}>
              <button class="item-select" onclick={() => selectLookForEdit(lk)}>
                <div class="item-title">
                  <strong>{lk.name}</strong>
                  {#if assignedCount > 0}
                    <span class="badge live">{assignedCount} screen{assignedCount > 1 ? "s" : ""}</span>
                  {/if}
                  {#if lk.transparent_background}
                    <span class="badge alpha">ALPHA</span>
                  {/if}
                  {#if lk.lower_third_mode}
                    <span class="badge lt">L3</span>
                  {/if}
                </div>
                <div class="item-sub">
                  <span>{lk.enabled_layers.length}/7 layers</span>
                  {#if tplNameBadge}
                    <span>· Template: {tplNameBadge}</span>
                  {/if}
                  <span>· Blend: {lk.slide_blend_mode}</span>
                </div>
              </button>
              <div class="item-actions">
                <button class="btn xs" onclick={() => startNewLook(lk)}>Duplicate</button>
                {#if !isProtected}
                  <button
                    class="btn xs danger"
                    disabled={busy}
                    onclick={() => void removeLook(lk)}
                  >
                    Delete
                  </button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      </div>

      <div class="card">
        <div class="card-head">
          <h3>{editingLookId ? "Edit Look" : "Create New Look"}</h3>
          {#if editingLookId}
            <div class="bulk-route">
              <button class="btn xs" disabled={busy} onclick={() => void applyLookToKind("Audience")}>
                Apply → All Audience
              </button>
              <button class="btn xs" disabled={busy} onclick={() => void applyLookToKind("Stage")}>
                Apply → All Stage
              </button>
            </div>
          {/if}
        </div>

        <div class="form-grid">
          <label>
            Look Name
            <input aria-label="Look Name" bind:value={lookName} placeholder="e.g. Livestream Lower-Third" />
          </label>
          <label>
            Slide Blend Mode
            <select aria-label="Slide Blend Mode" bind:value={lookBlendMode}>
              <option value="Normal">Normal</option>
              <option value="Multiply">Multiply</option>
              <option value="Screen">Screen</option>
              <option value="Additive">Additive</option>
            </select>
          </label>
          <label>
            Slide Template Override
            <select aria-label="Slide Template Override" bind:value={lookTemplateId}>
              <option value="">None (Use Slide's Own Layout)</option>
              {#each templatesList as tpl (tpl.id)}
                <option value={tpl.id}>{tpl.name} ({tpl.category})</option>
              {/each}
            </select>
          </label>
          <label>
            Mask Override
            <select aria-label="Mask Override" bind:value={lookMaskOverride}>
              <option value="inherit">Inherit Global Mask</option>
              <option value="None">Force None</option>
              <option value="SoftVignette">Soft Vignette</option>
              <option value="RoundedFrame">Rounded Frame</option>
              <option value="LowerThirdBand">Lower Third Band</option>
            </select>
          </label>
        </div>

        <label>
          Description
          <input
            aria-label="Look Description"
            bind:value={lookDescription}
            placeholder="Optional notes about which sanctuary/broadcast output uses this Look"
          />
        </label>

        <div class="layer-matrix">
          <span class="field-title">7-Layer Visibility Matrix (checked layers render on this output):</span>
          <div class="layer-checks">
            {#each ALL_LAYERS as layer (layer.kind)}
              <label class="check-pill" class:checked={lookLayers.includes(layer.kind)}>
                <input
                  type="checkbox"
                  checked={lookLayers.includes(layer.kind)}
                  onchange={() => toggleLookLayer(layer.kind)}
                />
                <span>{layer.code} {layer.label}</span>
              </label>
            {/each}
          </div>
        </div>

        <div class="flag-row">
          <label class="inline-check">
            <input type="checkbox" bind:checked={lookLowerThird} />
            Lower-Third Fallback Mode
          </label>
          <label class="inline-check">
            <input type="checkbox" bind:checked={lookTransparentBg} />
            Transparent Alpha Background (NDI / DeckLink Key+Fill)
          </label>
        </div>

        <div class="form-footer">
          <button class="btn primary" disabled={busy} onclick={() => void saveLook()}>
            {editingLookId ? "Save Look Live" : "Create Look"}
          </button>
        </div>
      </div>
    </div>
  {:else}
    <div class="two-col templates-grid">
      <div class="card">
        <div class="card-head">
          <h3>Templates Library ({filteredTemplates.length})</h3>
          <button class="btn sm primary" onclick={() => startNewTemplate()}>+ New Template</button>
        </div>

        <div class="cat-pills">
          {#each TEMPLATE_CATEGORIES as cat (cat)}
            <button
              class="cat-pill"
              class:active={templateCategoryFilter === cat}
              onclick={() => (templateCategoryFilter = cat)}
            >
              {cat}
            </button>
          {/each}
        </div>

        <div class="tpl-cards">
          {#each filteredTemplates as tpl (tpl.id)}
            <div class="tpl-card" class:selected={editingTemplateId === tpl.id}>
              <button class="tpl-thumb" onclick={() => selectTemplateForEdit(tpl)}>
                {#if tpl.accent_bar_color}
                  <div class="pv-accent" style={accentBarStyle(tpl)}></div>
                {/if}
                <div class="pv-box" style={previewBoxStyle(tpl.text_bounds, tpl.text_style, 0.13)}>
                  Amazing grace! How sweet the sound
                </div>
                {#if tpl.subtitle_bounds && tpl.subtitle_style}
                  <div
                    class="pv-box"
                    style={previewBoxStyle(tpl.subtitle_bounds, tpl.subtitle_style, 0.13)}
                  >
                    CCLI #22025
                  </div>
                {/if}
              </button>
              <div class="tpl-card-foot">
                <div class="tpl-meta">
                  <strong>{tpl.name}</strong>
                  <span class="cat-tag">{tpl.category}</span>
                </div>
                <div class="tpl-actions">
                  <button
                    class="btn xs primary"
                    disabled={busy}
                    onclick={() => void applyTemplateToActiveSlide(tpl.id, tpl.name)}
                  >
                    Apply Slide
                  </button>
                  <button
                    class="btn xs"
                    disabled={busy}
                    onclick={() => void applyTemplateToDeck(tpl.id, tpl.name)}
                  >
                    Apply Deck
                  </button>
                  <button class="btn xs" onclick={() => startNewTemplate(tpl)}>Copy</button>
                  <button
                    class="btn xs danger"
                    disabled={busy}
                    onclick={() => void removeTemplate(tpl)}
                  >
                    Del
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>

      <div class="card">
        <div class="card-head">
          <h3>{editingTemplateId ? "Edit Template" : "Create New Template"}</h3>
          <div class="quick-layouts">
            <button class="btn xs" onclick={() => applyTemplateLayoutPreset("center")}>Center</button>
            <button class="btn xs" onclick={() => applyTemplateLayoutPreset("lower-third")}>
              Lower Third
            </button>
            <button class="btn xs" onclick={() => applyTemplateLayoutPreset("upper-third")}>
              Upper Third
            </button>
            <button class="btn xs" onclick={() => applyTemplateLayoutPreset("left-col")}>
              Left Bold
            </button>
            <button class="btn xs" onclick={() => applyTemplateLayoutPreset("scripture")}>
              Scripture
            </button>
          </div>
        </div>

        <div class="live-stage" aria-label="Live Template 16:9 Preview">
          {#if livePreviewTemplate.accent_bar_color}
            <div class="pv-accent" style={accentBarStyle(livePreviewTemplate)}></div>
          {/if}
          <div
            class="pv-box"
            style={previewBoxStyle(
              livePreviewTemplate.text_bounds,
              livePreviewTemplate.text_style,
              0.24,
            )}
          >
            Amazing grace! How sweet the sound<br />That saved a wretch like me
          </div>
          {#if livePreviewTemplate.subtitle_bounds && livePreviewTemplate.subtitle_style}
            <div
              class="pv-box"
              style={previewBoxStyle(
                livePreviewTemplate.subtitle_bounds,
                livePreviewTemplate.subtitle_style,
                0.24,
              )}
            >
              PSALM 100:1-2 · CCLI #22025
            </div>
          {/if}
        </div>

        <div class="form-grid">
          <label>
            Template Name
            <input aria-label="Template Name" bind:value={tplName} placeholder="e.g. Livestream Lower Bar" />
          </label>
          <label>
            Category
            <select aria-label="Template Category" bind:value={tplCategory}>
              <option value="Lyrics">Lyrics</option>
              <option value="Lower Third">Lower Third</option>
              <option value="Scripture">Scripture</option>
              <option value="Announcement">Announcement</option>
              <option value="Custom">Custom</option>
            </select>
          </label>
          <label>
            Font Family
            <select aria-label="Font Family" bind:value={tplFontFamily}>
              <option value="sans-serif">Sans-Serif</option>
              <option value="serif">Serif</option>
              <option value="monospace">Monospace</option>
            </select>
          </label>
          <label>
            Font Size (pt)
            <input aria-label="Template Font Size" type="number" min="18" max="160" bind:value={tplFontSize} />
          </label>
          <label>
            H-Align
            <select aria-label="Horizontal Align" bind:value={tplHAlign}>
              <option value="Left">Left</option>
              <option value="Center">Center</option>
              <option value="Right">Right</option>
            </select>
          </label>
          <label>
            V-Align
            <select aria-label="Vertical Align" bind:value={tplVAlign}>
              <option value="Top">Top</option>
              <option value="Middle">Middle</option>
              <option value="Bottom">Bottom</option>
            </select>
          </label>
        </div>

        <div class="coords-row">
          <label>
            Box X
            <input aria-label="Box X" type="number" bind:value={tplBoundsX} />
          </label>
          <label>
            Box Y
            <input aria-label="Box Y" type="number" bind:value={tplBoundsY} />
          </label>
          <label>
            Width
            <input aria-label="Box Width" type="number" min="100" max="1920" bind:value={tplBoundsW} />
          </label>
          <label>
            Height
            <input aria-label="Box Height" type="number" min="60" max="1080" bind:value={tplBoundsH} />
          </label>
        </div>

        <div class="coords-row">
          <label>
            Text Color
            <input aria-label="Text Color" type="color" bind:value={tplTextHex} />
          </label>
          <label>
            Stroke Color
            <input aria-label="Stroke Color" type="color" bind:value={tplStrokeHex} />
          </label>
          <label>
            Stroke px
            <input aria-label="Stroke Width" type="number" min="0" max="12" step="0.5" bind:value={tplStrokeWidth} />
          </label>
          <label>
            Backdrop
            <input aria-label="Backdrop Color" type="color" bind:value={tplBackdropHex} />
          </label>
          <label>
            Pill Alpha ({tplBackdropAlpha}%)
            <input
              aria-label="Backdrop Opacity"
              type="range"
              min="0"
              max="100"
              bind:value={tplBackdropAlpha}
            />
          </label>
        </div>

        <div class="flag-row">
          <label class="inline-check">
            <input type="checkbox" bind:checked={tplBold} />
            Bold
          </label>
          <label class="inline-check">
            <input type="checkbox" bind:checked={tplItalic} />
            Italic
          </label>
          <label class="inline-check">
            <input type="checkbox" bind:checked={tplAutoShrink} />
            Auto-Shrink
          </label>
          <label class="inline-check">
            <input type="checkbox" bind:checked={tplHasAccentBar} />
            Accent Bar
          </label>
          {#if tplHasAccentBar}
            <input aria-label="Accent Bar Color" type="color" bind:value={tplAccentHex} />
          {/if}
          <label class="inline-check">
            <input type="checkbox" bind:checked={tplHasSubtitle} />
            Subtitle Box
          </label>
        </div>

        {#if tplHasSubtitle}
          <div class="coords-row">
            <label>
              Subtitle Y
              <input aria-label="Subtitle Y" type="number" bind:value={tplSubY} />
            </label>
            <label>
              Subtitle H
              <input aria-label="Subtitle Height" type="number" min="40" bind:value={tplSubH} />
            </label>
            <label>
              Subtitle Size
              <input aria-label="Subtitle Font Size" type="number" min="16" max="96" bind:value={tplSubFontSize} />
            </label>
            <label>
              Subtitle Color
              <input aria-label="Subtitle Color" type="color" bind:value={tplSubHex} />
            </label>
          </div>
        {/if}

        <div class="form-footer">
          <label class="inline-select">
            Default Mask:
            <select aria-label="Template Default Mask" bind:value={tplDefaultMask}>
              <option value="None">None</option>
              <option value="SoftVignette">Soft Vignette</option>
              <option value="RoundedFrame">Rounded Frame</option>
              <option value="LowerThirdBand">Lower Third Band</option>
            </select>
          </label>
          <button class="btn primary" disabled={busy} onclick={() => void saveTemplate()}>
            {editingTemplateId ? "Save Template Live" : "Create Template"}
          </button>
        </div>
      </div>
    </div>
  {/if}
</section>

<style>
  .lt-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    overflow-y: auto;
    height: 100%;
  }

  .lt-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .lt-head h2 {
    margin: 0;
    font-size: 16px;
  }

  .sub {
    margin: 3px 0 0;
    font-size: 12px;
    color: var(--text-1);
  }

  .mode-switch {
    display: flex;
    gap: 4px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    padding: 3px;
    border-radius: 7px;
  }

  .mode-btn {
    padding: 5px 11px;
    border-radius: 5px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-1);
    font-size: 12px;
    font-weight: 600;
  }

  .mode-btn.active {
    background: var(--accent, #2563eb);
    color: #fff;
  }

  .banner {
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 12px;
  }

  .banner.error {
    background: rgba(239, 68, 68, 0.16);
    border: 1px solid rgba(239, 68, 68, 0.5);
    color: #fecaca;
  }

  .banner.ok {
    background: rgba(16, 185, 129, 0.16);
    border: 1px solid rgba(16, 185, 129, 0.45);
    color: #a7f3d0;
  }

  .card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .card-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .card-head h3 {
    margin: 0;
    font-size: 13px;
  }

  .preset-save,
  .bulk-route,
  .quick-layouts {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .preset-save input {
    min-width: 220px;
  }

  .preset-chips {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .chip-label {
    font-size: 11px;
    color: var(--text-1);
  }

  .preset-chip {
    display: inline-flex;
    align-items: center;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 999px;
    overflow: hidden;
  }

  .preset-recall {
    border: none;
    background: transparent;
    color: var(--text-0);
    padding: 3px 9px;
    font-size: 11px;
    font-weight: 600;
  }

  .preset-recall:hover {
    background: rgba(59, 130, 246, 0.22);
  }

  .preset-del {
    border: none;
    border-left: 1px solid var(--border);
    background: transparent;
    color: var(--text-1);
    padding: 3px 7px;
    font-size: 11px;
  }

  .preset-del:hover {
    color: #fecaca;
    background: rgba(239, 68, 68, 0.22);
  }

  .routing-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 8px;
  }

  .route-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  .route-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }

  .kind-pill {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    padding: 2px 5px;
    border-radius: 4px;
    background: rgba(37, 99, 235, 0.28);
    color: #93c5fd;
  }

  .kind-pill.stage {
    background: rgba(13, 148, 136, 0.28);
    color: #5eead4;
  }

  .two-col {
    display: grid;
    grid-template-columns: 1fr 1.25fr;
    gap: 12px;
  }

  .templates-grid {
    grid-template-columns: 1fr 1.2fr;
  }

  .item-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .item-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  .item-row.selected {
    border-color: #3b82f6;
    background: rgba(37, 99, 235, 0.14);
  }

  .item-select {
    flex: 1;
    border: none;
    background: transparent;
    color: inherit;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 0;
  }

  .item-title {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
    font-size: 12px;
  }

  .item-sub {
    display: flex;
    gap: 5px;
    flex-wrap: wrap;
    font-size: 10px;
    color: var(--text-1);
  }

  .badge {
    font-size: 9px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 4px;
  }

  .badge.live {
    background: rgba(16, 185, 129, 0.22);
    color: #6ee7b7;
  }

  .badge.alpha {
    background: rgba(168, 85, 247, 0.25);
    color: #d8b4fe;
  }

  .badge.lt {
    background: rgba(59, 130, 246, 0.25);
    color: #93c5fd;
  }

  .item-actions {
    display: flex;
    gap: 5px;
  }

  .form-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 8px;
  }

  .coords-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(92px, 1fr));
    gap: 8px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--text-1);
  }

  input,
  select {
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
    font-size: 12px;
  }

  input[type="color"] {
    padding: 2px;
    height: 30px;
    width: 100%;
  }

  .layer-matrix {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field-title {
    font-size: 11px;
    color: var(--text-1);
  }

  .layer-checks {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .check-pill {
    flex-direction: row;
    align-items: center;
    gap: 5px;
    padding: 4px 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-1);
    font-size: 11px;
    cursor: pointer;
  }

  .check-pill.checked {
    border-color: #3b82f6;
    background: rgba(37, 99, 235, 0.22);
    color: var(--text-0);
  }

  .flag-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
  }

  .inline-check {
    flex-direction: row;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--text-0);
  }

  .inline-select {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }

  .form-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }

  .cat-pills {
    display: flex;
    gap: 5px;
    flex-wrap: wrap;
  }

  .cat-pill {
    padding: 3px 9px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-1);
    font-size: 11px;
  }

  .cat-pill.active {
    background: rgba(37, 99, 235, 0.28);
    border-color: #3b82f6;
    color: #fff;
  }

  .tpl-cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 8px;
  }

  .tpl-card {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-2);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .tpl-card.selected {
    border-color: #3b82f6;
  }

  .tpl-thumb {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    border: none;
    padding: 0;
    background: linear-gradient(135deg, #0c1b36, #143652);
    overflow: hidden;
    cursor: pointer;
  }

  .live-stage {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    border-radius: 7px;
    border: 1px solid #2b354b;
    background: linear-gradient(135deg, #0b1830, #123e56);
    overflow: hidden;
  }

  .pv-accent {
    position: absolute;
    width: 0.7%;
    z-index: 2;
  }

  .pv-box {
    position: absolute;
    display: flex;
    flex-direction: column;
    padding: 2px 5px;
    overflow: hidden;
    box-sizing: border-box;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.85);
    z-index: 3;
  }

  .tpl-card-foot {
    padding: 7px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .tpl-meta {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 4px;
    font-size: 11px;
  }

  .cat-tag {
    font-size: 9px;
    color: var(--text-1);
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 5px;
  }

  .tpl-actions {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }

  .btn {
    padding: 6px 11px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 12px;
    font-weight: 600;
  }

  .btn.sm {
    padding: 4px 9px;
    font-size: 11px;
  }

  .btn.xs {
    padding: 3px 7px;
    font-size: 10px;
  }

  .btn.primary {
    background: #2563eb;
    border-color: #3b82f6;
    color: #fff;
  }

  .btn.danger {
    background: rgba(239, 68, 68, 0.16);
    border-color: rgba(239, 68, 68, 0.45);
    color: #fecaca;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
