export interface ContextMenuItem {
  id: string;
  label: string;
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  separator?: boolean;
  children?: ContextMenuItem[];
}

export interface ContextMenuState {
  open: boolean;
  x: number;
  y: number;
  title: string | null;
  items: ContextMenuItem[];
  onSelect: ((id: string) => void) | null;
}

export const contextMenuState = $state<ContextMenuState>({
  open: false,
  x: 0,
  y: 0,
  title: null,
  items: [],
  onSelect: null,
});

const MENU_EST_WIDTH = 240;
const MENU_ITEM_HEIGHT = 28;
const MENU_BASE_HEIGHT = 44;

export function openContextMenu(
  event: MouseEvent,
  items: ContextMenuItem[],
  onSelect: (id: string) => void,
  title?: string,
) {
  event.preventDefault();
  event.stopPropagation();
  if (items.length === 0) {
    closeContextMenu();
    return;
  }

  const vw = typeof window !== "undefined" ? window.innerWidth : 1280;
  const vh = typeof window !== "undefined" ? window.innerHeight : 800;
  const estHeight = MENU_BASE_HEIGHT + items.length * MENU_ITEM_HEIGHT;

  contextMenuState.open = true;
  contextMenuState.x = Math.max(8, Math.min(event.clientX, Math.max(8, vw - MENU_EST_WIDTH - 8)));
  contextMenuState.y = Math.max(8, Math.min(event.clientY, Math.max(8, vh - estHeight - 8)));
  contextMenuState.title = title ?? null;
  contextMenuState.items = items;
  contextMenuState.onSelect = onSelect;
}

export function closeContextMenu() {
  contextMenuState.open = false;
  contextMenuState.items = [];
  contextMenuState.title = null;
  contextMenuState.onSelect = null;
}
