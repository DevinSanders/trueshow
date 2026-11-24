<script lang="ts">
  import {
    closeContextMenu,
    contextMenuState,
    type ContextMenuItem,
  } from "./contextMenuStore.svelte";

  let openSubmenuId = $state<string | null>(null);

  const flipSubmenuLeft = $derived(
    typeof window !== "undefined" && contextMenuState.x + 440 > window.innerWidth,
  );

  function isEditableTarget(target: EventTarget | null): boolean {
    const el = target as HTMLElement | null;
    if (!el) return false;
    const tag = el.tagName;
    return tag === "INPUT" || tag === "TEXTAREA" || Boolean(el.isContentEditable);
  }

  function closeAll() {
    openSubmenuId = null;
    closeContextMenu();
  }

  function handleWindowContextMenu(event: MouseEvent) {
    // Component-level context menu handlers call preventDefault() + stopPropagation().
    // Everywhere else outside text inputs, suppress the generic browser/WebView menu.
    if (event.defaultPrevented) return;
    if (isEditableTarget(event.target)) {
      closeAll();
      return;
    }
    event.preventDefault();
    closeAll();
  }

  function handleWindowClick(event: MouseEvent) {
    if (!contextMenuState.open) return;
    const target = event.target as HTMLElement | null;
    if (!target?.closest("[data-testid='context-menu']")) {
      closeAll();
    }
  }

  function handleWindowKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape" && contextMenuState.open) {
      closeAll();
    }
  }

  function runItem(item: ContextMenuItem) {
    if (item.disabled) return;
    if (item.children && item.children.length > 0) {
      openSubmenuId = openSubmenuId === item.id ? null : item.id;
      return;
    }
    const handler = contextMenuState.onSelect;
    closeAll();
    handler?.(item.id);
  }

  function hoverItem(item: ContextMenuItem) {
    if (item.children && item.children.length > 0 && !item.disabled) {
      openSubmenuId = item.id;
    } else {
      openSubmenuId = null;
    }
  }
</script>

<svelte:window
  oncontextmenu={handleWindowContextMenu}
  onclick={handleWindowClick}
  onkeydown={handleWindowKeyDown}
  onblur={closeAll}
/>

{#if contextMenuState.open}
  <div
    class="ctx-menu"
    role="menu"
    aria-label={contextMenuState.title ?? "Context menu"}
    data-testid="context-menu"
    style={`left:${contextMenuState.x}px;top:${contextMenuState.y}px;`}
    oncontextmenu={(e) => e.preventDefault()}
    tabindex="-1"
  >
    {#if contextMenuState.title}
      <div class="ctx-title">{contextMenuState.title}</div>
    {/if}
    {#each contextMenuState.items as item (item.id)}
      {#if item.separator}
        <div class="ctx-sep" role="separator"></div>
      {/if}
      <div class="ctx-item-wrap" role="none" onmouseenter={() => hoverItem(item)}>
        <button
          type="button"
          class="ctx-item"
          class:danger={item.danger}
          class:sub-open={openSubmenuId === item.id}
          role="menuitem"
          aria-haspopup={item.children?.length ? "menu" : undefined}
          aria-expanded={item.children?.length ? openSubmenuId === item.id : undefined}
          disabled={item.disabled}
          onclick={() => runItem(item)}
        >
          <span class="ctx-label">{item.label}</span>
          {#if item.children?.length}
            <span class="ctx-arrow" aria-hidden="true">▸</span>
          {:else if item.shortcut}
            <span class="ctx-shortcut">{item.shortcut}</span>
          {/if}
        </button>

        {#if item.children?.length && openSubmenuId === item.id}
          <div
            class="ctx-submenu"
            class:flip-left={flipSubmenuLeft}
            role="menu"
            aria-label={`${item.label} submenu`}
            data-testid={`submenu-${item.id}`}
          >
            {#each item.children as child (child.id)}
              {#if child.separator}
                <div class="ctx-sep" role="separator"></div>
              {/if}
              <button
                type="button"
                class="ctx-item"
                class:danger={child.danger}
                role="menuitem"
                disabled={child.disabled}
                onclick={() => runItem(child)}
              >
                <span class="ctx-label">{child.label}</span>
                {#if child.shortcut}
                  <span class="ctx-shortcut">{child.shortcut}</span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .ctx-menu,
  .ctx-submenu {
    min-width: 210px;
    max-width: 280px;
    padding: 4px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.62);
    display: flex;
    flex-direction: column;
    gap: 1px;
    user-select: none;
  }

  .ctx-menu {
    position: fixed;
    z-index: 200;
  }

  .ctx-item-wrap {
    position: relative;
  }

  .ctx-submenu {
    position: absolute;
    top: -4px;
    left: calc(100% + 2px);
    z-index: 210;
  }

  .ctx-submenu.flip-left {
    left: auto;
    right: calc(100% + 2px);
  }

  .ctx-title {
    padding: 4px 8px 5px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-1);
    border-bottom: 1px solid var(--border);
    margin-bottom: 3px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctx-sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }

  .ctx-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 5px 8px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-0);
    font-size: 12px;
    text-align: left;
    white-space: nowrap;
  }

  .ctx-item:hover:not(:disabled),
  .ctx-item.sub-open {
    background: var(--accent);
    color: #fff;
  }

  .ctx-item.danger {
    color: #fca5a5;
  }

  .ctx-item.danger:hover:not(:disabled) {
    background: #b91c1c;
    color: #fff;
  }

  .ctx-item:disabled {
    opacity: 0.42;
    cursor: not-allowed;
  }

  .ctx-label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ctx-shortcut,
  .ctx-arrow {
    font-size: 11px;
    color: var(--text-1);
    flex-shrink: 0;
  }

  .ctx-item:hover:not(:disabled) .ctx-shortcut,
  .ctx-item:hover:not(:disabled) .ctx-arrow,
  .ctx-item.sub-open .ctx-arrow {
    color: rgba(255, 255, 255, 0.9);
  }
</style>
