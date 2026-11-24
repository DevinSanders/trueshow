<script lang="ts">
  import { untrack } from "svelte";
  import userGuideRaw from "../../../../docs/USER_GUIDE.md?raw";
  import { parseGuideSections } from "./markdown";

  interface Props {
    initialAnchor?: string | null;
    onClose?: () => void;
  }

  let { initialAnchor = null, onClose }: Props = $props();

  // Bundle all docs/images/*.png assets directly inside the offline app build
  const rawImageModules = import.meta.glob<string>("../../../../docs/images/*.png", {
    eager: true,
    import: "default",
  });

  const imageMap: Record<string, string> = {};
  for (const [fullPath, url] of Object.entries(rawImageModules)) {
    const fileName = fullPath.split("/").pop();
    if (fileName) {
      imageMap[`images/${fileName}`] = url;
    }
  }

  const guide = parseGuideSections(userGuideRaw, imageMap);

  let searchQuery = $state("");
  let activeSectionId = $state<string>(guide.sections[0]?.id ?? "");
  let lightbox = $state<{ src: string; alt: string } | null>(null);

  const filteredSections = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return guide.sections;
    return guide.sections.filter((s) => s.plainText.includes(q));
  });

  function jumpToAnchor(anchorId: string) {
    activeSectionId = anchorId;
    if (searchQuery.trim().length > 0 && !filteredSections.some((s) => s.id === anchorId)) {
      searchQuery = "";
    }
    queueMicrotask(() => {
      const target = document.getElementById(anchorId);
      if (target && typeof target.scrollIntoView === "function") {
        target.scrollIntoView({ behavior: "smooth", block: "start" });
      }
    });
  }

  $effect(() => {
    const anchor = initialAnchor;
    if (anchor) {
      untrack(() => jumpToAnchor(anchor));
    }
  });

  function handleContentClick(event: MouseEvent) {
    const target = event.target as HTMLElement | null;
    if (!target) return;

    const anchorEl = target.closest<HTMLElement>("[data-guide-anchor]");
    if (anchorEl) {
      event.preventDefault();
      const slug = anchorEl.getAttribute("data-guide-anchor");
      if (slug) jumpToAnchor(slug);
      return;
    }

    const imgEl = target.closest<HTMLElement>("[data-lightbox-src]");
    if (imgEl) {
      const src = imgEl.getAttribute("data-lightbox-src");
      const alt = imgEl.getAttribute("data-lightbox-alt") ?? "OpenPresenter Screenshot";
      if (src) lightbox = { src, alt };
    }
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape" && lightbox) {
      lightbox = null;
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

<section class="ug-shell" data-testid="user-guide-panel">
  <aside class="ug-sidebar" aria-label="User Guide Table of Contents">
    <div class="ug-side-head">
      <h2>User Guide</h2>
      <span class="ug-badge">Offline</span>
    </div>

    <div class="ug-search-row">
      <input
        type="search"
        aria-label="Search User Guide"
        placeholder="Search topics, menus, shortcuts..."
        bind:value={searchQuery}
      />
      {#if searchQuery}
        <button type="button" class="ug-clear" onclick={() => (searchQuery = "")} aria-label="Clear search">
          ×
        </button>
      {/if}
    </div>

    <nav class="ug-toc">
      {#each filteredSections as sec (sec.id)}
        <button
          type="button"
          class="ug-toc-item"
          class:active={activeSectionId === sec.id}
          onclick={() => jumpToAnchor(sec.id)}
        >
          {sec.title}
        </button>
      {/each}
      {#if filteredSections.length === 0}
        <p class="ug-empty">No guide sections match "{searchQuery}".</p>
      {/if}
    </nav>
  </aside>

  <div class="ug-reader">
    <header class="ug-top">
      <div>
        <h1>{guide.title}</h1>
        <p class="ug-sub">
          Built-in offline reference · {guide.sections.length} sections · Click any screenshot to enlarge
        </p>
      </div>
      {#if onClose}
        <button type="button" class="btn-return" onclick={onClose}>Back to Show Deck</button>
      {/if}
    </header>

    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="ug-body" onclick={handleContentClick}>
      {#if !searchQuery.trim()}
        <div class="ug-intro">
          {@html guide.introHtml}
        </div>
      {/if}

      {#each filteredSections as sec (sec.id)}
        <article class="ug-section" id={sec.id}>
          <h2 class="ug-section-title">{sec.title}</h2>
          <div class="ug-section-body">
            {@html sec.html}
          </div>
        </article>
      {/each}
    </div>
  </div>
</section>

{#if lightbox}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="ug-lightbox"
    role="dialog"
    aria-modal="true"
    aria-label={lightbox.alt}
    tabindex="-1"
    onclick={(e) => {
      if (e.target === e.currentTarget) lightbox = null;
    }}
  >
    <div class="ug-lightbox-inner">
      <div class="ug-lightbox-bar">
        <span>{lightbox.alt}</span>
        <button type="button" class="btn-return" onclick={() => (lightbox = null)}>Close (Esc)</button>
      </div>
      <img src={lightbox.src} alt={lightbox.alt} />
    </div>
  </div>
{/if}

<style>
  .ug-shell {
    display: grid;
    grid-template-columns: 260px 1fr;
    gap: 12px;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .ug-sidebar {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }

  .ug-side-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .ug-side-head h2 {
    margin: 0;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-0);
  }

  .ug-badge {
    font-size: 10px;
    font-weight: 700;
    padding: 2px 6px;
    border-radius: 999px;
    background: rgba(34, 197, 94, 0.18);
    color: #86efac;
    border: 1px solid rgba(34, 197, 94, 0.35);
  }

  .ug-search-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .ug-search-row input {
    flex: 1;
    min-width: 0;
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font-size: 12px;
  }

  .ug-clear {
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    border-radius: 5px;
    padding: 4px 8px;
    font-size: 12px;
  }

  .ug-toc {
    display: flex;
    flex-direction: column;
    gap: 3px;
    overflow-y: auto;
    flex: 1;
    padding-right: 2px;
  }

  .ug-toc-item {
    text-align: left;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-1);
    padding: 6px 8px;
    border-radius: 6px;
    font-size: 12px;
    line-height: 1.3;
  }

  .ug-toc-item:hover {
    background: var(--bg-2);
    color: var(--text-0);
  }

  .ug-toc-item.active {
    background: rgba(59, 130, 246, 0.22);
    border-color: rgba(59, 130, 246, 0.55);
    color: #fff;
    font-weight: 600;
  }

  .ug-empty {
    font-size: 12px;
    color: var(--text-1);
    padding: 8px;
  }

  .ug-reader {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 16px 22px;
    overflow-y: auto;
    min-height: 0;
  }

  .ug-top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 14px;
  }

  .ug-top h1 {
    margin: 0;
    font-size: 20px;
    color: var(--text-0);
  }

  .ug-sub {
    margin: 4px 0 0 0;
    font-size: 12px;
    color: var(--text-1);
  }

  .btn-return {
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }

  .btn-return:hover {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .ug-section {
    padding: 14px 0 18px 0;
    border-bottom: 1px solid var(--border);
  }

  .ug-section:last-child {
    border-bottom: none;
  }

  .ug-section-title {
    margin: 0 0 10px 0;
    font-size: 16px;
    color: #93c5fd;
  }

  .ug-body :global(h3) {
    margin: 16px 0 8px 0;
    font-size: 14px;
    color: var(--text-0);
  }

  .ug-body :global(p),
  .ug-body :global(li) {
    font-size: 13px;
    line-height: 1.55;
    color: var(--text-0);
  }

  .ug-body :global(ul),
  .ug-body :global(ol) {
    padding-left: 20px;
    margin: 6px 0 10px 0;
  }

  .ug-body :global(li) {
    margin-bottom: 4px;
  }

  .ug-body :global(code) {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 12px;
    color: #bfdbfe;
  }

  .ug-body :global(.ug-pre) {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px 12px;
    overflow-x: auto;
    margin: 8px 0 12px 0;
  }

  .ug-body :global(.ug-pre code) {
    border: none;
    padding: 0;
    background: transparent;
  }

  .ug-body :global(.ug-quote) {
    margin: 10px 0;
    padding: 9px 12px;
    border-left: 3px solid var(--accent);
    background: rgba(59, 130, 246, 0.1);
    border-radius: 0 6px 6px 0;
    font-size: 12.5px;
    color: #dbeafe;
  }

  .ug-body :global(.ug-table-wrap) {
    overflow-x: auto;
    margin: 10px 0 14px 0;
  }

  .ug-body :global(.ug-table) {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  .ug-body :global(.ug-table th),
  .ug-body :global(.ug-table td) {
    border: 1px solid var(--border);
    padding: 7px 9px;
    text-align: left;
    vertical-align: top;
  }

  .ug-body :global(.ug-table th) {
    background: var(--bg-2);
    color: var(--text-0);
    font-weight: 700;
  }

  .ug-body :global(.ug-figure) {
    margin: 12px 0 16px 0;
    padding: 8px;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  .ug-body :global(.ug-figure img) {
    width: 100%;
    height: auto;
    display: block;
    border-radius: 5px;
    cursor: zoom-in;
  }

  .ug-body :global(.ug-figure figcaption) {
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-1);
    text-align: center;
  }

  .ug-body :global(a) {
    color: #60a5fa;
    text-decoration: underline;
  }

  .ug-body :global(hr) {
    border: none;
    border-top: 1px solid var(--border);
    margin: 14px 0;
  }

  .ug-lightbox {
    position: fixed;
    inset: 0;
    z-index: 120;
    background: rgba(5, 8, 15, 0.88);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }

  .ug-lightbox-inner {
    max-width: 96vw;
    max-height: 92vh;
    display: flex;
    flex-direction: column;
    gap: 10px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.7);
  }

  .ug-lightbox-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-0);
  }

  .ug-lightbox-inner img {
    max-width: 100%;
    max-height: calc(92vh - 70px);
    object-fit: contain;
    border-radius: 6px;
  }
</style>
