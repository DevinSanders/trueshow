<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Arrangement, SceneState } from "./types";

  interface Props {
    scene: SceneState;
    onSceneChange: (next: SceneState) => void;
    onError?: (msg: string) => void;
  }

  let { scene, onSceneChange, onError }: Props = $props();

  let title = $state("Great Is Thy Faithfulness");
  let author = $state("Thomas Chisholm");
  let ccliNumber = $state("18723");
  let maxLinesPerSlide = $state(4);
  let rawLyrics = $state(
    `[Verse 1]\nGreat is Thy faithfulness, O God my Father // Grande es Tu fidelidad\nThere is no shadow of turning with Thee\nThou changest not, Thy compassions they fail not\nAs Thou hast been Thou forever wilt be\n\n[Chorus]\nGreat is Thy faithfulness! Great is Thy faithfulness!\nMorning by morning new mercies I see\nAll I have needed Thy hand hath provided\nGreat is Thy faithfulness, Lord, unto me!`,
  );

  let newArrName = $state("Full Band");
  let draftOrder = $state<string[]>([]);
  let busy = $state(false);
  let notice = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  const availableGroups = $derived.by(() => {
    const source = scene.master_deck && scene.master_deck.length > 0 ? scene.master_deck : scene.deck;
    const seen = new Set<string>();
    const out: string[] = [];
    for (const s of source) {
      if (s.group && !seen.has(s.group)) {
        seen.add(s.group);
        out.push(s.group);
      }
    }
    return out;
  });

  const arrangements = $derived(scene.active_arrangements ?? []);

  async function saveQuickLyrics(loadToDeck: boolean) {
    errorMsg = null;
    notice = null;
    if (!title.trim()) {
      errorMsg = "Please enter a song title.";
      return;
    }
    busy = true;
    try {
      const maybeState = await invoke<SceneState | null>("parse_and_save_quick_lyrics", {
        libraryId: "worship-songs",
        title: title.trim(),
        author: author.trim() || null,
        ccliNumber: ccliNumber.trim() || null,
        rawLyrics,
        maxLinesPerSlide: Number(maxLinesPerSlide),
        loadToDeck,
      });
      if (maybeState) {
        onSceneChange(maybeState);
        notice = `Parsed & loaded "${title.trim()}" (${maybeState.deck.length} slides) to Cue Deck.`;
      } else {
        notice = `Saved "${title.trim()}" to Worship Songs library.`;
      }
    } catch (err) {
      errorMsg = `Failed to parse Quick Lyrics: ${String(err)}`;
      onError?.(errorMsg);
    } finally {
      busy = false;
    }
  }

  function appendGroup(group: string) {
    draftOrder = [...draftOrder, group];
  }

  function removeOrderItem(idx: number) {
    draftOrder = draftOrder.filter((_, i) => i !== idx);
  }

  async function selectArrangement(arrId: string | null) {
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const next = await invoke<SceneState>("select_arrangement", { arrangementId: arrId });
      onSceneChange(next);
      notice = arrId ? "Switched live song arrangement." : "Restored Master Slide Order.";
    } catch (err) {
      errorMsg = `Failed to switch arrangement: ${String(err)}`;
      onError?.(errorMsg);
    } finally {
      busy = false;
    }
  }

  async function saveCustomArrangement() {
    errorMsg = null;
    notice = null;
    if (!newArrName.trim() || draftOrder.length === 0) {
      errorMsg = "Enter an arrangement name and click at least one group chip to build the order.";
      return;
    }
    busy = true;
    try {
      const created: Arrangement = {
        id: `arr-${Date.now()}`,
        name: newArrName.trim(),
        group_order: [...draftOrder],
      };
      const updated = [...arrangements, created];
      const next = await invoke<SceneState>("save_arrangements", {
        arrangements: updated,
        activeId: created.id,
      });
      onSceneChange(next);
      draftOrder = [];
      notice = `Created & activated arrangement "${created.name}" (${next.deck.length} cued slides).`;
    } catch (err) {
      errorMsg = `Failed to save arrangement: ${String(err)}`;
      onError?.(errorMsg);
    } finally {
      busy = false;
    }
  }
</script>

<section class="ql-panel" data-testid="quick-lyrics-panel">
  <header class="ql-head">
    <div>
      <h2>Quick Lyrics / Reflow & Non-Destructive Arrangements</h2>
      <p class="sub">
        Paste lyrics using <code>[Verse 1]</code>, <code>[Chorus]</code>, <code>[Bridge]</code> headers (use <code>//</code> for dual-language lines), auto-split by line count, and arrange groups without duplicating slides.
      </p>
    </div>
  </header>

  {#if errorMsg}
    <div class="banner err" role="alert">{errorMsg}</div>
  {/if}
  {#if notice}
    <div class="banner ok" role="status">{notice}</div>
  {/if}

  <div class="two-col">
    <div class="card">
      <h3>Quick Lyrics & Reflow</h3>
      <div class="meta-row">
        <label>
          Song Title
          <input aria-label="Quick Lyrics Title" bind:value={title} />
        </label>
        <label>
          Author
          <input aria-label="Quick Lyrics Author" bind:value={author} />
        </label>
        <label>
          CCLI #
          <input aria-label="Quick Lyrics CCLI" bind:value={ccliNumber} />
        </label>
        <label>
          Max Lines / Slide
          <select aria-label="Max Lines Per Slide" bind:value={maxLinesPerSlide}>
            <option value={2}>2 lines</option>
            <option value={4}>4 lines</option>
            <option value={6}>6 lines</option>
            <option value={8}>8 lines</option>
          </select>
        </label>
      </div>
      <textarea aria-label="Quick Lyrics Input" rows="10" bind:value={rawLyrics}></textarea>
      <div class="actions">
        <button class="btn primary" disabled={busy} onclick={() => void saveQuickLyrics(true)}>
          Parse & Load to Cue Deck
        </button>
        <button class="btn" disabled={busy} onclick={() => void saveQuickLyrics(false)}>
          Save to Library Only
        </button>
      </div>
    </div>

    <div class="card">
      <h3>Live Deck Arrangements</h3>
      <div class="arr-bar">
        <button
          class="chip"
          class:active={!scene.active_arrangement_id}
          disabled={busy}
          onclick={() => void selectArrangement(null)}
        >
          Master Order
        </button>
        {#each arrangements as arr (arr.id)}
          <button
            class="chip"
            class:active={scene.active_arrangement_id === arr.id}
            disabled={busy}
            onclick={() => void selectArrangement(arr.id)}
          >
            {arr.name} ({arr.group_order.length})
          </button>
        {/each}
      </div>

      <div class="builder">
        <label>
          New Arrangement Name
          <input aria-label="Arrangement Name" bind:value={newArrName} placeholder="e.g. Short Service" />
        </label>
        <div class="group-palette">
          <span class="muted">Click groups to append to sequence:</span>
          <div class="chips">
            {#each availableGroups as g (g)}
              <button class="chip add" onclick={() => appendGroup(g)}>+ {g}</button>
            {/each}
          </div>
        </div>
        <div class="sequence-box">
          {#if draftOrder.length === 0}
            <span class="muted">Sequence empty — click groups above (e.g. Verse 1 → Chorus → Verse 2 → Chorus).</span>
          {:else}
            {#each draftOrder as g, i (g + "-" + i)}
              <button class="seq-pill" onclick={() => removeOrderItem(i)} title="Click to remove">
                {i + 1}. {g} ×
              </button>
            {/each}
          {/if}
        </div>
        <button class="btn primary" disabled={busy} onclick={() => void saveCustomArrangement()}>
          Save & Activate Arrangement
        </button>
      </div>
    </div>
  </div>
</section>

<style>
  .ql-panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .ql-head h2 {
    margin: 0;
    font-size: 14px;
  }

  .sub {
    margin: 3px 0 0;
    font-size: 11px;
    color: var(--text-1);
  }

  .two-col {
    display: grid;
    grid-template-columns: 1.3fr 1fr;
    gap: 12px;
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

  .card h3 {
    margin: 0;
    font-size: 13px;
  }

  .meta-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
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
  select,
  textarea {
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font-size: 12px;
    font-family: inherit;
  }

  .actions,
  .arr-bar,
  .chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .builder {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .group-palette {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .sequence-box {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    min-height: 44px;
    padding: 8px;
    background: var(--bg-0);
    border: 1px dashed var(--border);
    border-radius: 6px;
    align-items: center;
  }

  .chip {
    padding: 4px 9px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 11px;
    font-weight: 600;
  }

  .chip.active {
    background: #2563eb;
    border-color: #3b82f6;
    color: #fff;
  }

  .chip.add {
    border-color: #3b82f6;
  }

  .seq-pill {
    padding: 3px 8px;
    border-radius: 5px;
    border: 1px solid rgba(56, 189, 248, 0.5);
    background: rgba(37, 99, 235, 0.25);
    color: #e0f2fe;
    font-size: 11px;
  }

  .muted {
    font-size: 11px;
    color: var(--text-1);
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
    padding: 6px 11px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 12px;
    font-weight: 600;
    align-self: flex-start;
  }

  .btn.primary {
    background: #2563eb;
    border-color: #3b82f6;
    color: #fff;
  }
</style>
