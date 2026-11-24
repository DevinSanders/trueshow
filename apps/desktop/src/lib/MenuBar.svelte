<script lang="ts">
  export type WorkspaceTab =
    | "show"
    | "scripture"
    | "lyrics"
    | "overlays"
    | "screens"
    | "stage"
    | "looks"
    | "editor"
    | "broadcast"
    | "ecosystem"
    | "diagnostics";

  interface MenuItem {
    id: string;
    label: string;
    shortcut?: string;
    checked?: boolean;
    disabled?: boolean;
    danger?: boolean;
    separator?: boolean;
    action: () => void;
  }

  interface MenuGroup {
    id: string;
    label: string;
    items: MenuItem[];
  }

  interface Props {
    activeTab: WorkspaceTab;
    connected: boolean;
    frozen: boolean;
    crashCount: number;
    showLibraryDrawer: boolean;
    showLayerDock: boolean;
    showMediaDrawer: boolean;
    onSelectTab: (tab: WorkspaceTab) => void;
    onToggleLibraryDrawer: () => void;
    onToggleLayerDock: () => void;
    onToggleMediaDrawer: () => void;
    onUndo: () => void;
    onRedo: () => void;
    onToggleFreeze: () => void;
    onReconnect: () => void;
    onReloadUi: () => void;
    onShutdownEngine: () => void;
    onShutdownAll: () => void;
  }

  let {
    activeTab,
    connected,
    frozen,
    crashCount,
    showLibraryDrawer,
    showLayerDock,
    showMediaDrawer,
    onSelectTab,
    onToggleLibraryDrawer,
    onToggleLayerDock,
    onToggleMediaDrawer,
    onUndo,
    onRedo,
    onToggleFreeze,
    onReconnect,
    onReloadUi,
    onShutdownEngine,
    onShutdownAll,
  }: Props = $props();

  let openMenuId = $state<string | null>(null);

  const tabTitles: Record<WorkspaceTab, string> = {
    show: "Show Deck",
    scripture: "Scripture",
    lyrics: "Quick Lyrics",
    overlays: "Overlays & Messages",
    screens: "Screens & Monitors",
    stage: "Stage Designer",
    looks: "Looks & Templates",
    editor: "Slide Editor",
    broadcast: "Broadcast I/O",
    ecosystem: "Show Control & Sync",
    diagnostics: "Diagnostics",
  };

  const menus = $derived<MenuGroup[]>([
    {
      id: "file",
      label: "File",
      items: [
        { id: "file-show", label: "Show Deck", checked: activeTab === "show", action: () => onSelectTab("show") },
        { id: "file-scripture", label: "Scripture Lookup", checked: activeTab === "scripture", action: () => onSelectTab("scripture") },
        { id: "file-lyrics", label: "Quick Lyrics & Arrangements", checked: activeTab === "lyrics", action: () => onSelectTab("lyrics") },
        { id: "file-import", label: "Import & .opbundle Backup...", separator: true, action: () => onSelectTab("ecosystem") },
        { id: "file-reconnect", label: "Reconnect Engine", separator: true, action: onReconnect },
        { id: "file-reload", label: "Reload UI (Keep Live)", action: onReloadUi },
        { id: "file-stop", label: "Stop Engine", disabled: !connected, danger: true, separator: true, action: onShutdownEngine },
        { id: "file-quit", label: "Shutdown All", danger: true, action: onShutdownAll },
      ],
    },
    {
      id: "edit",
      label: "Edit",
      items: [
        { id: "edit-undo", label: "Undo", shortcut: "Ctrl+Z", disabled: !connected, action: onUndo },
        { id: "edit-redo", label: "Redo", shortcut: "Ctrl+Y", disabled: !connected, action: onRedo },
        { id: "edit-slide", label: "Slide Editor", checked: activeTab === "editor", separator: true, action: () => onSelectTab("editor") },
        { id: "edit-overlays", label: "Overlays & Messages", checked: activeTab === "overlays", action: () => onSelectTab("overlays") },
      ],
    },
    {
      id: "view",
      label: "View",
      items: [
        { id: "view-show", label: "Show Workspace", checked: activeTab === "show", action: () => onSelectTab("show") },
        { id: "view-scripture", label: "Scripture Drawer", checked: activeTab === "scripture", action: () => onSelectTab("scripture") },
        { id: "view-lyrics", label: "Quick Lyrics", checked: activeTab === "lyrics", action: () => onSelectTab("lyrics") },
        { id: "view-lib", label: "Libraries & Playlists Drawer", checked: showLibraryDrawer, separator: true, action: onToggleLibraryDrawer },
        { id: "view-layers", label: "Layer Manager Dock", checked: showLayerDock, action: onToggleLayerDock },
        { id: "view-media", label: "Media Bin Drawer", checked: showMediaDrawer, action: onToggleMediaDrawer },
      ],
    },
    {
      id: "screens",
      label: "Screens",
      items: [
        { id: "scr-config", label: "Configure Screens & Monitors", checked: activeTab === "screens", action: () => onSelectTab("screens") },
        { id: "scr-looks", label: "Looks & Templates", checked: activeTab === "looks", action: () => onSelectTab("looks") },
        { id: "scr-stage", label: "Stage Layout Designer", checked: activeTab === "stage", action: () => onSelectTab("stage") },
        { id: "scr-broadcast", label: "Broadcast I/O & LAN Remote", checked: activeTab === "broadcast", separator: true, action: () => onSelectTab("broadcast") },
      ],
    },
    {
      id: "tools",
      label: "Tools",
      items: [
        { id: "tools-control", label: "Control, Timers, Macros & Sync", checked: activeTab === "ecosystem", action: () => onSelectTab("ecosystem") },
        {
          id: "tools-diag",
          label: crashCount > 0 ? `Diagnostics (${crashCount})` : "Diagnostics & Logs",
          checked: activeTab === "diagnostics",
          separator: true,
          action: () => onSelectTab("diagnostics"),
        },
      ],
    },
  ]);

  function toggleMenu(id: string) {
    openMenuId = openMenuId === id ? null : id;
  }

  function hoverMenu(id: string) {
    if (openMenuId !== null && openMenuId !== id) {
      openMenuId = id;
    }
  }

  function runItem(item: MenuItem) {
    if (item.disabled) return;
    openMenuId = null;
    item.action();
  }

  function handleWindowClick(event: MouseEvent) {
    if (openMenuId === null) return;
    const target = event.target as HTMLElement | null;
    if (!target?.closest(".menubar-menus")) {
      openMenuId = null;
    }
  }

  function handleWindowKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape" && openMenuId !== null) {
      openMenuId = null;
    }
  }
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeyDown} />

<header class="menubar" data-testid="menu-bar">
  <div class="menubar-left">
    <strong class="brand-name">OpenPresenter</strong>

    <div class="menubar-menus" aria-label="Application menu" role="menubar">
      {#each menus as group (group.id)}
        <div class="menu-wrap">
          <button
            type="button"
            class="menu-trigger"
            class:open={openMenuId === group.id}
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={openMenuId === group.id}
            onclick={() => toggleMenu(group.id)}
            onmouseenter={() => hoverMenu(group.id)}
          >
            {group.label}
          </button>

          {#if openMenuId === group.id}
            <div class="menu-dropdown" role="menu" aria-label={`${group.label} menu`}>
              {#each group.items as item (item.id)}
                {#if item.separator}
                  <div class="menu-sep" role="separator"></div>
                {/if}
                <button
                  type="button"
                  class="menu-item"
                  class:danger={item.danger}
                  role="menuitem"
                  disabled={item.disabled}
                  onclick={() => runItem(item)}
                >
                  <span class="menu-check" aria-hidden="true">{item.checked ? "✓" : ""}</span>
                  <span class="menu-label">{item.label}</span>
                  {#if item.shortcut}
                    <span class="menu-shortcut">{item.shortcut}</span>
                  {/if}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>

  <div class="menubar-right">
    <span class="view-pill" data-testid="active-view-label">{tabTitles[activeTab]}</span>
    {#if activeTab !== "show"}
      <button type="button" class="quick-btn" onclick={() => onSelectTab("show")}>
        Show
      </button>
    {/if}
    <button
      type="button"
      class="quick-btn"
      class:frozen
      onclick={onToggleFreeze}
      disabled={!connected}
      title="Freeze Live Outputs (Hotkey: F)"
    >
      {frozen ? "Unfreeze" : "Freeze"}
    </button>
  </div>
</header>

<style>
  .menubar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    flex-shrink: 0;
    user-select: none;
    position: relative;
    z-index: 40;
  }

  .menubar-left,
  .menubar-menus,
  .menubar-right {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }

  .brand-name {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--text-0);
    padding-right: 8px;
    border-right: 1px solid var(--border);
    margin-right: 4px;
    white-space: nowrap;
  }

  .menu-wrap {
    position: relative;
  }

  .menu-trigger {
    padding: 4px 8px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    color: var(--text-1);
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
  }

  .menu-trigger:hover,
  .menu-trigger.open {
    background: var(--bg-2);
    border-color: var(--border);
    color: var(--text-0);
  }

  .menu-dropdown {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    min-width: 230px;
    padding: 4px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.55);
    display: flex;
    flex-direction: column;
    gap: 1px;
    z-index: 60;
  }

  .menu-sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 6px;
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

  .menu-item:hover:not(:disabled) {
    background: var(--accent);
    color: #fff;
  }

  .menu-item.danger {
    color: #fca5a5;
  }

  .menu-item.danger:hover:not(:disabled) {
    background: #b91c1c;
    color: #fff;
  }

  .menu-item:disabled {
    opacity: 0.42;
    cursor: not-allowed;
  }

  .menu-check {
    width: 14px;
    text-align: center;
    font-weight: 700;
    flex-shrink: 0;
  }

  .menu-label {
    flex: 1;
  }

  .menu-shortcut {
    font-size: 11px;
    color: var(--text-1);
    margin-left: 14px;
  }

  .menu-item:hover:not(:disabled) .menu-shortcut {
    color: rgba(255, 255, 255, 0.85);
  }

  .view-pill {
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }

  .quick-btn {
    padding: 3px 9px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }

  .quick-btn.frozen {
    background: rgba(245, 158, 11, 0.25);
    border-color: var(--warn);
    color: #fde68a;
  }

  .quick-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
