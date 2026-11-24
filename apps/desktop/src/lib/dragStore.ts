export type DragPayload =
  | { type: "song"; songId: string; title: string }
  | { type: "playlist-cue"; fromIdx: number }
  | { type: "media"; blake3_hash: string; kind: string; file_name: string };

let activePayload: DragPayload | null = null;

export function startAppDrag(event: DragEvent, payload: DragPayload): void {
  activePayload = payload;
  const dt = event.dataTransfer;
  if (!dt) return;
  try {
    dt.effectAllowed = "copyMove";
  } catch {
    // Some headless test environments expose a partial DataTransfer mock
  }
  if (payload.type === "song") {
    dt.setData("application/x-openpresenter-song", payload.songId);
    dt.setData("text/plain", payload.title || payload.songId);
  } else if (payload.type === "playlist-cue") {
    dt.setData("application/x-openpresenter-playlist-idx", String(payload.fromIdx));
    dt.setData("text/plain", String(payload.fromIdx));
  } else {
    dt.setData("application/x-openpresenter-media", JSON.stringify(payload));
    dt.setData("text/plain", payload.file_name);
  }
}

export function clearAppDrag(): void {
  activePayload = null;
}

export function allowDrop(
  event: DragEvent,
  acceptedTypes: DragPayload["type"][],
  effect: "copy" | "move" = "copy",
): boolean {
  if (activePayload && !acceptedTypes.includes(activePayload.type)) {
    return false;
  }
  event.preventDefault();
  if (event.dataTransfer) {
    try {
      event.dataTransfer.dropEffect = effect;
    } catch {
      // Partial mock in unit tests
    }
  }
  return true;
}

export function readDroppedSongId(event: DragEvent): string | null {
  const fromStore = activePayload?.type === "song" ? activePayload.songId : null;
  const fromDt = event.dataTransfer?.getData("application/x-openpresenter-song") || null;
  activePayload = null;
  return fromDt || fromStore;
}

export function readDroppedPlaylistIndex(event: DragEvent): number | null {
  const fromStore = activePayload?.type === "playlist-cue" ? activePayload.fromIdx : null;
  const raw = event.dataTransfer?.getData("application/x-openpresenter-playlist-idx");
  activePayload = null;
  if (raw !== undefined && raw !== "") {
    const parsed = Number(raw);
    if (!Number.isNaN(parsed)) return parsed;
  }
  return fromStore;
}

export function readDroppedMedia(
  event: DragEvent,
): { blake3_hash: string; kind?: string; file_name?: string } | null {
  const fromStore = activePayload?.type === "media" ? activePayload : null;
  const raw = event.dataTransfer?.getData("application/x-openpresenter-media");
  activePayload = null;
  if (raw) {
    try {
      const parsed = JSON.parse(raw) as {
        blake3_hash?: string;
        kind?: string;
        file_name?: string;
      };
      if (parsed.blake3_hash) {
        return {
          blake3_hash: parsed.blake3_hash,
          kind: parsed.kind,
          file_name: parsed.file_name,
        };
      }
    } catch {
      return null;
    }
  }
  if (fromStore) {
    return {
      blake3_hash: fromStore.blake3_hash,
      kind: fromStore.kind,
      file_name: fromStore.file_name,
    };
  }
  return null;
}
