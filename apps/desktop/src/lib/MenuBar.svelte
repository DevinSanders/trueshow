<script lang="ts">
  import type { ControlSection } from "./ShowControlPanel.svelte";

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
    | "diagnostics"
    | "guide";

  interface MenuItem {
    id: string;
    label: string;
    shortcut?: string;
    checked?: boolean;
    disabled?: boolean;
    danger?: boolean;
    separator?: boolean;
  }

  interface MenuGroup {
    id: string;
    label: string;
    items: MenuItem[];
  }

  interface Props {
    activeTab: WorkspaceTab;
    activeControlSection?: ControlSection;
    connected: boolean;
    frozen: boolean;
    crashCount: number;
    showLibraryDrawer: boolean;
    showLayerDock: boolean;
    showMediaDrawer: boolean;
    onSelectTab: (tab: WorkspaceTab, section?: ControlSection, anchor?: string) => void;
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
    activeControlSection = "timers",
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
    guide: "User Guide",
  };

  const sectionTitles: Record<ControlSection, string> = {
    timers: "Live Timers",
    automation: "Macros & Schedule",
    import: "Import & Planning Center",
    ccli: "CCLI Attribution & Reports",
    backup: "Library Backup (.tsbundle)",
  };

  const activeViewTitle = $derived(
    activeTab === "ecosystem" ? sectionTitles[activeControlSection] : tabTitles[activeTab],
  );

  const menus = $derived<MenuGroup[]>([
    {
      id: "file",
      label: "File",
      items: [
        { id: "tab:show", label: "Show Deck", checked: activeTab === "show" },
        { id: "tab:scripture", label: "Scripture Lookup", checked: activeTab === "scripture" },
        { id: "tab:lyrics", label: "Quick Lyrics & Arrangements", checked: activeTab === "lyrics" },
        { id: "tool:import", label: "Import Files & Planning Center...", separator: true },
        { id: "tool:backup", label: "Backup & Restore (.tsbundle)..." },
        { id: "reconnect", label: "Reconnect Engine", separator: true },
        { id: "reload", label: "Reload UI (Keep Live)" },
        { id: "stop-engine", label: "Stop Engine", disabled: !connected, danger: true, separator: true },
        { id: "shutdown-all", label: "Shutdown All", danger: true },
      ],
    },
    {
      id: "edit",
      label: "Edit",
      items: [
        { id: "undo", label: "Undo", shortcut: "Ctrl+Z", disabled: !connected },
        { id: "redo", label: "Redo", shortcut: "Ctrl+Y", disabled: !connected },
        { id: "tab:editor", label: "Slide Editor", checked: activeTab === "editor", separator: true },
        { id: "tab:overlays", label: "Overlays & Messages", checked: activeTab === "overlays" },
      ],
    },
    {
      id: "view",
      label: "View",
      items: [
        { id: "tab:show", label: "Show Workspace", checked: activeTab === "show" },
        { id: "tab:scripture", label: "Scripture Drawer", checked: activeTab === "scripture" },
        { id: "tab:lyrics", label: "Quick Lyrics", checked: activeTab === "lyrics" },
        { id: "toggle-lib", label: "Libraries & Playlists Drawer", checked: showLibraryDrawer, separator: true },
        { id: "toggle-layers", label: "Layer Manager Dock", checked: showLayerDock },
        { id: "toggle-media", label: "Media Bin Drawer", checked: showMediaDrawer },
      ],
    },
    {
      id: "screens",
      label: "Screens",
      items: [
        { id: "tab:screens", label: "Configure Screens & Monitors", checked: activeTab === "screens" },
        { id: "tab:looks", label: "Looks & Templates", checked: activeTab === "looks" },
        { id: "tab:stage", label: "Stage Layout Designer", checked: activeTab === "stage" },
        { id: "tab:broadcast", label: "Broadcast I/O & LAN Remote", checked: activeTab === "broadcast", separator: true },
      ],
    },
    {
      id: "tools",
      label: "Tools",
      items: [
        { id: "tool:timers", label: "Live Timers", checked: activeTab === "ecosystem" && activeControlSection === "timers" },
        { id: "tool:automation", label: "Macros, Calendar & OSC", checked: activeTab === "ecosystem" && activeControlSection === "automation" },
        { id: "tool:import", label: "Import & Planning Center", checked: activeTab === "ecosystem" && activeControlSection === "import", separator: true },
        { id: "tool:ccli", label: "CCLI Attribution & Reports", checked: activeTab === "ecosystem" && activeControlSection === "ccli" },
        { id: "tool:backup", label: "Backup & Restore (.tsbundle)", checked: activeTab === "ecosystem" && activeControlSection === "backup" },
        {
          id: "tab:diagnostics",
          label: crashCount > 0 ? `Diagnostics (${crashCount})` : "Diagnostics & Logs",
          checked: activeTab === "diagnostics",
          separator: true,
        },
      ],
    },
    {
      id: "help",
      label: "Help",
      items: [
        { id: "tab:guide", label: "User Guide", checked: activeTab === "guide" },
        { id: "guide:shortcuts", label: "Keyboard Shortcuts" },
        {
          id: "tab:diagnostics",
          label: crashCount > 0 ? `Diagnostics (${crashCount})` : "Diagnostics & Logs",
          checked: activeTab === "diagnostics",
          separator: true,
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
    if (item.id.startsWith("tab:")) {
      onSelectTab(item.id.slice(4) as WorkspaceTab);
      return;
    }
    if (item.id.startsWith("tool:")) {
      onSelectTab("ecosystem", item.id.slice(5) as ControlSection);
      return;
    }
    if (item.id === "guide:shortcuts") {
      onSelectTab("guide", undefined, "20-complete-keyboard-shortcuts-reference");
      return;
    }
    switch (item.id) {
      case "toggle-lib": onToggleLibraryDrawer(); break;
      case "toggle-layers": onToggleLayerDock(); break;
      case "toggle-media": onToggleMediaDrawer(); break;
      case "undo": onUndo(); break;
      case "redo": onRedo(); break;
      case "reconnect": onReconnect(); break;
      case "reload": onReloadUi(); break;
      case "stop-engine": onShutdownEngine(); break;
      case "shutdown-all": onShutdownAll(); break;
    }
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
    <div class="brand-lockup">
      <svg
        class="brand-logo"
        width="18"
        height="18"
        viewBox="0 0 64 64"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden="true"
      >
        <defs>
          <linearGradient id="mb-ts-bg" x1="6" y1="4" x2="58" y2="60" gradientUnits="userSpaceOnUse">
            <stop offset="0%" stop-color="#172554" />
            <stop offset="100%" stop-color="#0b1120" />
          </linearGradient>
          <linearGradient id="mb-ts-slide" x1="10" y1="20" x2="48" y2="52" gradientUnits="userSpaceOnUse">
            <stop offset="0%" stop-color="#38bdf8" />
            <stop offset="100%" stop-color="#2563eb" />
          </linearGradient>
        </defs>
        <rect width="64" height="64" rx="15" fill="url(#mb-ts-bg)" />
        <rect x="0.75" y="0.75" width="62.5" height="62.5" rx="14.25" stroke="#38bdf8" stroke-opacity="0.28" stroke-width="1.5" />
        <rect x="20" y="13" width="33" height="23" rx="4.5" fill="#1e3a8a" stroke="#60a5fa" stroke-opacity="0.35" stroke-width="1.2" />
        <rect x="15" y="19" width="35" height="25" rx="5" fill="#1d4ed8" fill-opacity="0.85" stroke="#7dd3fc" stroke-opacity="0.4" stroke-width="1.2" />
        <rect x="10" y="25" width="38" height="27" rx="5.5" fill="url(#mb-ts-slide)" />
        <path d="M25.5 32.8C25.5 31.8 26.6 31.2 27.4 31.7L36.2 37.4C37 37.9 37 39.1 36.2 39.6L27.4 45.3C26.6 45.8 25.5 45.2 25.5 44.2V32.8Z" fill="#f8fafc" />
      </svg>
      <strong class="brand-name">TrueShow</strong>
    </div>

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
    <span class="view-pill" data-testid="active-view-label">{activeViewTitle}</span>
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

  .brand-lockup {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding-right: 8px;
    border-right: 1px solid var(--border);
    margin-right: 4px;
    flex-shrink: 0;
  }

  .brand-logo {
    display: block;
    flex-shrink: 0;
    border-radius: 4px;
  }

  .brand-name {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--text-0);
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
