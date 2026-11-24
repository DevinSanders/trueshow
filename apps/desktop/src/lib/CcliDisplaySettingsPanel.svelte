<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { CcliDisplayConfig, CcliDisplayPlacement, SceneState } from "./types";

  interface Props {
    onSceneChange?: (next: SceneState) => void;
  }

  let { onSceneChange }: Props = $props();

  let enabled = $state(true);
  let placement = $state<CcliDisplayPlacement>("FirstSlide");
  let churchLicense = $state("");
  let template = $state(
    "{title} • {author}\n{copyright} • CCLI Song #{ccli_song} • CCLI License #{ccli_license}",
  );
  let fontSize = $state(24);
  let hAlign = $state<"Left" | "Center" | "Right">("Center");

  let statusMsg = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  const tokens = ["{title}", "{author}", "{copyright}", "{ccli_song}", "{ccli_license}"];

  const livePreview = $derived.by(() => {
    const lic = churchLicense.trim() || "1234567";
    return template
      .replaceAll("{title}", "Amazing Grace")
      .replaceAll("{author}", "John Newton")
      .replaceAll("{copyright}", "Public Domain")
      .replaceAll("{ccli_song}", "22025")
      .replaceAll("{ccli_license}", lic);
  });

  async function loadConfig() {
    try {
      const cfg = await invoke<CcliDisplayConfig>("get_ccli_display_config");
      if (!cfg) return;
      enabled = cfg.enabled;
      placement = cfg.placement;
      churchLicense = cfg.church_license_number ?? "";
      template = cfg.template || template;
      fontSize = cfg.font_size || 24;
      hAlign = cfg.h_align || "Center";
    } catch {
      // ignore offline mount errors
    }
  }

  function insertToken(tok: string) {
    template = template.endsWith(" ") || template.endsWith("\n") || template.length === 0
      ? `${template}${tok}`
      : `${template} • ${tok}`;
  }

  async function saveConfig() {
    errorMsg = null;
    statusMsg = null;
    if (!template.trim()) {
      errorMsg = "CCLI display template cannot be empty";
      return;
    }
    const config: CcliDisplayConfig = {
      enabled,
      placement,
      church_license_number: churchLicense.trim(),
      template: template.trim(),
      font_size: Math.max(14, Math.min(56, Number(fontSize) || 24)),
      text_color: { r: 0.95, g: 0.97, b: 1.0, a: 0.92 },
      background_color: { r: 0.03, g: 0.05, b: 0.09, a: 0.65 },
      bounds: { x: 80, y: 984, width: 1760, height: 76 },
      h_align: hAlign,
    };
    try {
      const next = await invoke<SceneState>("save_ccli_display_config", { config });
      if (next) onSceneChange?.(next);
      statusMsg = `Saved automated CCLI display (${enabled ? placement : "Off"}${churchLicense.trim() ? ` · License #${churchLicense.trim()}` : ""}).`;
    } catch (err) {
      errorMsg = String(err);
    }
  }

  onMount(() => {
    void loadConfig();
  });
</script>

<div class="ccli-card" data-testid="ccli-display-settings">
  <h3>Automated CCLI License & Song Attribution Display</h3>

  {#if errorMsg}
    <div class="alert err" role="alert">{errorMsg}</div>
  {/if}
  {#if statusMsg}
    <div class="alert ok" role="status">{statusMsg}</div>
  {/if}

  <div class="row">
    <label class="check-field">
      <input type="checkbox" aria-label="Enable Automated CCLI Display" bind:checked={enabled} />
      <span>Auto-Display Attribution</span>
    </label>

    <label class="field">
      <span>Show Automatically On</span>
      <select aria-label="CCLI Slide Placement" bind:value={placement}>
        <option value="FirstSlide">First Slide of Song</option>
        <option value="LastSlide">Last Slide of Song</option>
        <option value="FirstAndLastSlide">First & Last Slide</option>
        <option value="AllSlides">Every Song Slide</option>
        <option value="Disabled">Disabled</option>
      </select>
    </label>

    <label class="field">
      <span>Church CCLI License #</span>
      <input
        aria-label="Church CCLI License Number"
        placeholder="e.g. 1234567"
        bind:value={churchLicense}
      />
    </label>

    <label class="field sm">
      <span>Font (px)</span>
      <input aria-label="CCLI Font Size" type="number" min="14" max="56" bind:value={fontSize} />
    </label>

    <label class="field sm">
      <span>Align</span>
      <select aria-label="CCLI Alignment" bind:value={hAlign}>
        <option value="Left">Left</option>
        <option value="Center">Center</option>
        <option value="Right">Right</option>
      </select>
    </label>
  </div>

  <div class="token-row">
    <span>Insert Token:</span>
    {#each tokens as tok (tok)}
      <button type="button" class="tok-btn" onclick={() => insertToken(tok)}>{tok}</button>
    {/each}
  </div>

  <textarea
    aria-label="CCLI Display Template"
    rows="2"
    bind:value={template}
  ></textarea>

  <div class="preview-strip" style={`text-align:${hAlign.toLowerCase()};`} data-testid="ccli-preview">
    {livePreview}
  </div>

  <div class="row">
    <button type="button" class="btn primary" onclick={() => void saveConfig()}>
      Save & Apply CCLI Template Live
    </button>
  </div>
</div>

<style>
  .ccli-card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  h3 {
    margin: 0;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-1);
  }

  .row,
  .token-row {
    display: flex;
    gap: 8px;
    align-items: flex-end;
    flex-wrap: wrap;
  }

  .token-row {
    align-items: center;
    font-size: 11px;
    color: var(--text-1);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 11px;
    color: var(--text-1);
    flex: 1;
    min-width: 130px;
  }

  .field.sm {
    flex: 0 0 88px;
    min-width: 80px;
  }

  .check-field {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--text-0);
    padding-bottom: 5px;
  }

  input,
  select,
  textarea {
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 5px 8px;
    font: inherit;
    font-size: 12px;
  }

  .tok-btn {
    padding: 2px 7px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 11px;
  }

  .preview-strip {
    padding: 8px 12px;
    border-radius: 6px;
    background: #05080f;
    border: 1px solid var(--border);
    color: rgba(242, 247, 255, 0.92);
    font-size: 11px;
    white-space: pre-wrap;
    line-height: 1.35;
  }

  .btn {
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 12px;
    font-weight: 600;
  }

  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .alert {
    padding: 7px 10px;
    border-radius: 6px;
    font-size: 12px;
  }

  .alert.err {
    background: rgba(239, 68, 68, 0.18);
    color: #fecaca;
  }

  .alert.ok {
    background: rgba(34, 197, 94, 0.16);
    color: #bbf7d0;
  }
</style>
