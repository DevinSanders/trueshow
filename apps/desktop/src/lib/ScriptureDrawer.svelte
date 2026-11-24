<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { BibleVerseRecord, SceneState } from "./types";

  interface Props {
    onDeckLoaded: (next: SceneState) => void;
    onError?: (msg: string) => void;
  }

  interface ScriptureLookupResult {
    primary: BibleVerseRecord[];
    secondary: BibleVerseRecord[];
  }

  let { onDeckLoaded, onError }: Props = $props();

  let translations = $state<string[]>([]);
  let primaryTranslation = $state("KJV");
  let secondaryTranslation = $state("");
  let query = $state("John 3:16-17");
  let versesPerSlide = $state(1);
  let showVerseNumbers = $state(true);
  let primaryHits = $state<BibleVerseRecord[]>([]);
  let secondaryHits = $state<BibleVerseRecord[]>([]);
  let busy = $state(false);
  let notice = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  const QUICK_REFS = ["John 3:16-17", "Psalm 23:1-3", "Psalm 100:1-2", "Romans 8:28", "Numbers 6:24-26"];

  async function refreshTranslations() {
    try {
      const list = await invoke<string[]>("list_bible_translations");
      translations = Array.isArray(list) ? list : [];
      if (translations.length > 0 && !translations.includes(primaryTranslation)) {
        primaryTranslation = translations[0];
      }
    } catch {
      translations = [];
    }
  }

  async function searchScripture() {
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const res = await invoke<ScriptureLookupResult>("lookup_scripture", {
        translation: primaryTranslation || null,
        secondaryTranslation: secondaryTranslation || null,
        query,
        limit: 24,
      });
      primaryHits = Array.isArray(res?.primary) ? res.primary : [];
      secondaryHits = Array.isArray(res?.secondary) ? res.secondary : [];
    } catch (err) {
      errorMsg = `Scripture lookup failed: ${String(err)}`;
      onError?.(errorMsg);
    } finally {
      busy = false;
    }
  }

  async function presentPassage(saveToLibrary: boolean) {
    if (primaryHits.length === 0) {
      errorMsg = "No verses found to present. Try searching a reference like John 3:16.";
      return;
    }
    errorMsg = null;
    notice = null;
    busy = true;
    try {
      const next = await invoke<SceneState>("present_scripture", {
        primary: primaryHits,
        secondary: secondaryHits,
        versesPerSlide: Number(versesPerSlide),
        showVerseNumbers,
        saveToLibrary,
      });
      onDeckLoaded(next);
      notice = saveToLibrary
        ? `Saved & loaded ${next.deck.length} scripture slide(s) to Cue Deck.`
        : `Loaded ${next.deck.length} scripture slide(s) live to Cue Deck.`;
    } catch (err) {
      errorMsg = `Failed to present scripture: ${String(err)}`;
      onError?.(errorMsg);
    } finally {
      busy = false;
    }
  }

  function pickQuickRef(refStr: string) {
    query = refStr;
    void searchScripture();
  }

  onMount(() => {
    void refreshTranslations().then(() => searchScripture());
  });
</script>

<section class="scripture-panel" data-testid="scripture-panel">
  <header class="head">
    <div>
      <h2>Scripture / Bible Lookup (Offline FTS5 + Dual-Translation)</h2>
      <p class="sub">
        Jump directly by reference (e.g. <code>John 3:16-17</code>, <code>Ps 23</code>) or search keywords offline; optionally layer a second parallel translation.
      </p>
    </div>
    <div class="quick-refs">
      {#each QUICK_REFS as qr (qr)}
        <button class="chip" onclick={() => pickQuickRef(qr)}>{qr}</button>
      {/each}
    </div>
  </header>

  {#if errorMsg}
    <div class="banner err" role="alert">{errorMsg}</div>
  {/if}
  {#if notice}
    <div class="banner ok" role="status">{notice}</div>
  {/if}

  <div class="toolbar">
    <input
      aria-label="Scripture Reference or Keyword"
      placeholder="Reference (John 3:16-17) or keyword (shepherd)..."
      bind:value={query}
      onkeydown={(e) => {
        if (e.key === "Enter") void searchScripture();
      }}
    />
    <select aria-label="Primary Translation" bind:value={primaryTranslation} onchange={() => void searchScripture()}>
      {#each translations as tr (tr)}
        <option value={tr}>{tr}</option>
      {/each}
    </select>
    <select aria-label="Secondary Parallel Translation" bind:value={secondaryTranslation} onchange={() => void searchScripture()}>
      <option value="">No Parallel Translation</option>
      {#each translations as tr (tr)}
        <option value={tr}>Parallel: {tr}</option>
      {/each}
    </select>
    <label class="inline-opt">
      Verses/Slide:
      <select aria-label="Verses Per Slide" bind:value={versesPerSlide}>
        <option value={1}>1</option>
        <option value={2}>2</option>
        <option value={3}>3</option>
        <option value={4}>4</option>
      </select>
    </label>
    <label class="inline-opt">
      <input type="checkbox" bind:checked={showVerseNumbers} />
      Verse #
    </label>
    <button class="btn" disabled={busy} onclick={() => void searchScripture()}>Search</button>
    <button class="btn primary" disabled={busy} onclick={() => void presentPassage(false)}>
      Present Live
    </button>
    <button class="btn" disabled={busy} onclick={() => void presentPassage(true)}>
      Save & Present
    </button>
  </div>

  <div class="results">
    {#if primaryHits.length === 0}
      <p class="muted">No verses match "{query}". Import additional OSIS/USFX/Zefania XML Bibles in Control & Sync.</p>
    {:else}
      <ul class="verse-list">
        {#each primaryHits as v (v.translation + v.book + v.chapter + v.verse)}
          {@const parallel = secondaryHits.find(
            (s) => s.book === v.book && s.chapter === v.chapter && s.verse === v.verse,
          )}
          <li class="verse-row">
            <div class="verse-ref">
              <strong>{v.book} {v.chapter}:{v.verse}</strong>
              <span class="tr-pill">{v.translation}</span>
            </div>
            <p class="verse-txt">{v.text}</p>
            {#if parallel}
              <p class="verse-parallel">
                <span class="tr-pill sec">{parallel.translation}</span>
                {parallel.text}
              </p>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  .scripture-panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .head h2 {
    margin: 0;
    font-size: 14px;
  }

  .sub {
    margin: 3px 0 0;
    font-size: 11px;
    color: var(--text-1);
  }

  .quick-refs {
    display: flex;
    gap: 5px;
    flex-wrap: wrap;
  }

  .chip {
    padding: 3px 8px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 11px;
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px;
  }

  .toolbar input:not([type]),
  .toolbar select {
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
    font-size: 12px;
  }

  .toolbar input:not([type]) {
    flex: 1;
    min-width: 190px;
  }

  .inline-opt {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-1);
  }

  .results {
    flex: 1;
    overflow-y: auto;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px;
  }

  .verse-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .verse-row {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 10px;
  }

  .verse-ref {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    margin-bottom: 4px;
  }

  .tr-pill {
    font-size: 9px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(37, 99, 235, 0.25);
    color: #93c5fd;
  }

  .tr-pill.sec {
    background: rgba(245, 158, 11, 0.25);
    color: #fde68a;
  }

  .verse-txt {
    margin: 0;
    font-size: 13px;
    line-height: 1.4;
  }

  .verse-parallel {
    margin: 5px 0 0;
    font-size: 12px;
    color: #fde68a;
    font-style: italic;
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

  .muted {
    color: var(--text-1);
    font-size: 12px;
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

  .btn.primary {
    background: #2563eb;
    border-color: #3b82f6;
    color: #fff;
  }
</style>
