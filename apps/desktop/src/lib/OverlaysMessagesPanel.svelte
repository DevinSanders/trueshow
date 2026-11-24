<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { MessageTemplate, OverlayItem, SceneState } from "./types";

  interface Props {
    scene: SceneState;
    onSceneChange: (next: SceneState) => void;
    onError?: (msg: string) => void;
  }

  let { scene, onSceneChange, onError }: Props = $props();

  let notice = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);
  let busy = $state(false);

  // Overlay editor
  let ovId = $state<string | null>(null);
  let ovName = $state("Give Online Banner");
  let ovText = $state("Text GIVE to 555-0199 · grace.church/give");
  let ovX = $state(120);
  let ovY = $state(860);
  let ovW = $state(860);
  let ovH = $state(110);
  let ovLocked = $state(false);

  // Message templates & live token values
  let messageTemplates = $state<MessageTemplate[]>([]);
  let selectedMsgId = $state<string>("");
  let msgText = $state("Parent with Tag #{code}, please check Nursery Room {room}.");
  let msgAudience = $state(true);
  let msgStage = $state(true);
  let tokenValues = $state<Record<string, string>>({ code: "14", room: "B" });

  // Dynamic variables
  let varKey = $state("TeamA");
  let varVal = $state("10");

  const overlaysList = $derived(
    Object.values(scene.overlays ?? {}).sort((a, b) => a.name.localeCompare(b.name)),
  );
  const activeOverlayIds = $derived(scene.active_overlay_ids ?? []);
  const variablesEntries = $derived(Object.entries(scene.variables ?? {}));

  const extractedTokens = $derived.by(() => {
    const found = new Set<string>();
    const re = /\{([a-zA-Z0-9_]+)\}/g;
    let m: RegExpExecArray | null = re.exec(msgText);
    while (m) {
      found.add(m[1]);
      m = re.exec(msgText);
    }
    return Array.from(found);
  });

  function reportErr(err: unknown, prefix: string) {
    errorMsg = `${prefix}: ${String(err)}`;
    onError?.(errorMsg);
  }

  async function loadMessageTemplates() {
    try {
      const list = await invoke<MessageTemplate[]>("list_message_templates");
      messageTemplates = Array.isArray(list) ? list : [];
      if (messageTemplates.length > 0 && !selectedMsgId) {
        selectTemplate(messageTemplates[0]);
      }
    } catch {
      messageTemplates = [];
    }
  }

  function selectTemplate(tpl: MessageTemplate) {
    selectedMsgId = tpl.id;
    msgText = tpl.template_text;
    msgAudience = tpl.show_on_audience;
    msgStage = tpl.show_on_stage;
  }

  async function toggleOverlay(id: string) {
    errorMsg = null;
    notice = null;
    const active = !activeOverlayIds.includes(id);
    try {
      const next = await invoke<SceneState>("toggle_overlay_item", { overlayId: id, active });
      onSceneChange(next);
    } catch (err) {
      reportErr(err, "Failed to toggle overlay");
    }
  }

  async function saveOverlay() {
    errorMsg = null;
    notice = null;
    if (!ovName.trim() || !ovText.trim()) {
      errorMsg = "Overlay name and text are required.";
      return;
    }
    busy = true;
    try {
      const overlay: OverlayItem = {
        id: ovId ?? `ov-${Date.now()}`,
        name: ovName.trim(),
        text: ovText.trim(),
        bounds: { x: Number(ovX), y: Number(ovY), width: Number(ovW), height: Number(ovH) },
        style: {
          font_family: "sans-serif",
          font_size: 36,
          bold: true,
          italic: false,
          line_height: 1.2,
          color: { r: 1, g: 1, b: 1, a: 1 },
          stroke_color: { r: 0, g: 0, b: 0, a: 0.9 },
          stroke_width: 1.5,
          shadow_color: { r: 0, g: 0, b: 0, a: 0.6 },
          shadow_offset_x: 0,
          shadow_offset_y: 4,
          shadow_blur: 6,
          backdrop_color: { r: 0.06, g: 0.1, b: 0.18, a: 0.9 },
          backdrop_radius: 14,
          backdrop_padding: 18,
          h_align: "Left",
          v_align: "Middle",
          auto_shrink: true,
        },
        accent_color: { r: 0.22, g: 0.62, b: 1, a: 0.96 },
        locked: ovLocked,
        auto_clear_secs: null,
      };
      const next = await invoke<SceneState>("upsert_overlay", { overlay });
      onSceneChange(next);
      ovId = null;
      notice = `Saved overlay "${overlay.name}".`;
    } catch (err) {
      reportErr(err, "Failed to save overlay");
    } finally {
      busy = false;
    }
  }

  async function removeOverlay(id: string) {
    errorMsg = null;
    notice = null;
    try {
      const next = await invoke<SceneState>("delete_overlay", { overlayId: id });
      onSceneChange(next);
      notice = "Deleted overlay.";
    } catch (err) {
      reportErr(err, "Failed to delete overlay");
    }
  }

  async function triggerMessage() {
    errorMsg = null;
    notice = null;
    if (!msgText.trim()) {
      errorMsg = "Message text cannot be empty.";
      return;
    }
    busy = true;
    try {
      const tokens: Record<string, string> = {};
      for (const k of extractedTokens) {
        tokens[k] = tokenValues[k] ?? "";
      }
      const next = await invoke<SceneState>("trigger_message", {
        templateId: selectedMsgId || null,
        templateText: msgText,
        tokens,
        showOnAudience: msgAudience,
        showOnStage: msgStage,
      });
      onSceneChange(next);
      notice = "Message displayed live on Layer 6.";
    } catch (err) {
      reportErr(err, "Failed to show message");
    } finally {
      busy = false;
    }
  }

  async function clearMessage() {
    errorMsg = null;
    notice = null;
    try {
      const next = await invoke<SceneState>("clear_message");
      onSceneChange(next);
      notice = "Cleared live message.";
    } catch (err) {
      reportErr(err, "Failed to clear message");
    }
  }

  async function saveVariable(key: string, value: string | null) {
    errorMsg = null;
    notice = null;
    if (!key.trim()) {
      errorMsg = "Variable name is required.";
      return;
    }
    try {
      const next = await invoke<SceneState>("set_variable", { key: key.trim(), value });
      onSceneChange(next);
      notice = value === null ? `Removed variable {{var:${key.trim()}}}.` : `Updated {{var:${key.trim()}}} = ${value}.`;
    } catch (err) {
      reportErr(err, "Failed to update variable");
    }
  }

  function bumpNumericVar(key: string, current: string, delta: number) {
    const parsed = Number.parseInt(current, 10);
    const nextVal = String((Number.isFinite(parsed) ? parsed : 0) + delta);
    void saveVariable(key, nextVal);
  }

  onMount(() => {
    void loadMessageTemplates();
  });
</script>

<section class="om-panel" data-testid="overlays-messages-panel">
  <header>
    <h2>Overlays / Props (Layer 4), Tokenized Messages (Layer 6) & Live Variables</h2>
    <p class="sub">
      Stack multiple simultaneous overlays (lock persistent bugs across Clear All), fire Nursery/Alert tokenized messages, and update live <code>{"{{var:Name}}"}</code> tokens.
    </p>
  </header>

  {#if errorMsg}
    <div class="banner err" role="alert">{errorMsg}</div>
  {/if}
  {#if notice}
    <div class="banner ok" role="status">{notice}</div>
  {/if}

  <div class="grid">
    <div class="card">
      <h3>Layer 4 — Reusable Overlays & Props</h3>
      <ul class="list">
        {#each overlaysList as ov (ov.id)}
          {@const isOn = activeOverlayIds.includes(ov.id)}
          <li class="row-item" class:active={isOn}>
            <div class="meta">
              <strong>{ov.name}</strong>
              <span class="small">{ov.text}</span>
              {#if ov.locked}
                <span class="pill lock">LOCKED</span>
              {/if}
            </div>
            <div class="row-btns">
              <button class="btn xs" class:primary={isOn} onclick={() => void toggleOverlay(ov.id)}>
                {isOn ? "Hide" : "Show"}
              </button>
              <button class="btn xs danger" onclick={() => void removeOverlay(ov.id)}>Del</button>
            </div>
          </li>
        {/each}
      </ul>

      <div class="form">
        <input aria-label="Overlay Name" bind:value={ovName} placeholder="Overlay name" />
        <input aria-label="Overlay Text" bind:value={ovText} placeholder={"Display text (supports {{var:Name}})"} />
        <label class="check">
          <input type="checkbox" bind:checked={ovLocked} />
          Lock across Clear All
        </label>
        <button class="btn primary sm" disabled={busy} onclick={() => void saveOverlay()}>
          Save Overlay
        </button>
      </div>
    </div>

    <div class="card">
      <h3>Layer 6 — Tokenized Messages (Nursery / Alerts)</h3>
      <div class="chips">
        {#each messageTemplates as tpl (tpl.id)}
          <button
            class="chip"
            class:active={selectedMsgId === tpl.id}
            onclick={() => selectTemplate(tpl)}
          >
            {tpl.name}
          </button>
        {/each}
      </div>

      <textarea aria-label="Message Template Text" rows="2" bind:value={msgText}></textarea>

      {#if extractedTokens.length > 0}
        <div class="tokens">
          {#each extractedTokens as tok (tok)}
            <label>
              {`{${tok}}`}
              <input
                aria-label={`Token ${tok}`}
                value={tokenValues[tok] ?? ""}
                oninput={(e) => (tokenValues = { ...tokenValues, [tok]: e.currentTarget.value })}
              />
            </label>
          {/each}
        </div>
      {/if}

      <div class="flags">
        <label class="check"><input type="checkbox" bind:checked={msgAudience} /> Audience</label>
        <label class="check"><input type="checkbox" bind:checked={msgStage} /> Stage</label>
      </div>

      <div class="row-btns">
        <button class="btn primary sm" disabled={busy} onclick={() => void triggerMessage()}>
          Show Message Live
        </button>
        <button class="btn sm" onclick={() => void clearMessage()}>Clear Message</button>
      </div>
    </div>

    <div class="card wide">
      <h3>Dynamic Variables (use <code>{"{{var:Name}}"}</code> inside slides or overlays)</h3>
      <div class="var-add">
        <input aria-label="Variable Name" bind:value={varKey} placeholder="Variable name (e.g. TeamA)" />
        <input aria-label="Variable Value" bind:value={varVal} placeholder="Value (e.g. 12)" />
        <button class="btn primary sm" onclick={() => void saveVariable(varKey, varVal)}>
          Set Variable
        </button>
      </div>
      <div class="var-list">
        {#each variablesEntries as [k, v] (k)}
          <div class="var-chip">
            <code>{`{{var:${k}}}`}</code>
            <strong>{v}</strong>
            <button class="btn xs" onclick={() => bumpNumericVar(k, v, -1)}>-1</button>
            <button class="btn xs" onclick={() => bumpNumericVar(k, v, 1)}>+1</button>
            <button class="btn xs danger" onclick={() => void saveVariable(k, null)}>×</button>
          </div>
        {/each}
      </div>
    </div>
  </div>
</section>

<style>
  .om-panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    flex: 1;
    overflow-y: auto;
  }

  h2 {
    margin: 0;
    font-size: 14px;
  }

  h3 {
    margin: 0;
    font-size: 12px;
    text-transform: uppercase;
    color: var(--text-1);
  }

  .sub {
    margin: 3px 0 0;
    font-size: 11px;
    color: var(--text-1);
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .card.wide {
    grid-column: span 2;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 7px 9px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  .row-item.active {
    border-color: #22c55e;
    background: rgba(34, 197, 94, 0.14);
  }

  .meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
  }

  .small {
    font-size: 10px;
    color: var(--text-1);
  }

  .pill.lock {
    align-self: flex-start;
    font-size: 9px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(245, 158, 11, 0.25);
    color: #fde68a;
  }

  .form,
  .tokens {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .tokens {
    flex-direction: row;
    flex-wrap: wrap;
  }

  .tokens label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 11px;
    color: var(--text-1);
    flex: 1;
    min-width: 110px;
  }

  input:not([type="checkbox"]),
  textarea {
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
    font-size: 12px;
    font-family: inherit;
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
  }

  .chips,
  .flags,
  .row-btns,
  .var-add,
  .var-list {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    align-items: center;
  }

  .chip {
    padding: 3px 9px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 11px;
  }

  .chip.active {
    background: #2563eb;
    border-color: #3b82f6;
    color: #fff;
  }

  .var-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    font-size: 11px;
  }

  .banner {
    padding: 7px 10px;
    border-radius: 6px;
    font-size: 12px;
  }

  .banner.err {
    background: rgba(239, 68, 68, 0.16);
    color: #fecaca;
  }

  .banner.ok {
    background: rgba(16, 185, 129, 0.16);
    color: #a7f3d0;
  }

  .btn {
    padding: 5px 10px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 12px;
    font-weight: 600;
  }

  .btn.sm {
    padding: 5px 9px;
    font-size: 11px;
  }

  .btn.xs {
    padding: 2px 7px;
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
</style>
