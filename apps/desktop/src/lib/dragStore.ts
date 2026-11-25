export type DragPayload =
  | { type: "song"; songId: string; title: string }
  | { type: "playlist-cue"; fromIdx: number }
  | { type: "media"; blake3_hash: string; kind: string; file_name: string }
  | { type: "master-group"; group: string }
  | { type: "arrangement-slot"; fromIdx: number };

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
    dt.setData("application/x-trueshow-song", payload.songId);
    dt.setData("text/plain", payload.title || payload.songId);
  } else if (payload.type === "playlist-cue") {
    dt.setData("application/x-trueshow-playlist-idx", String(payload.fromIdx));
    dt.setData("text/plain", String(payload.fromIdx));
  } else if (payload.type === "master-group") {
    dt.setData("application/x-trueshow-master-group", payload.group);
    dt.setData("text/plain", payload.group);
  } else if (payload.type === "arrangement-slot") {
    dt.setData("application/x-trueshow-arr-idx", String(payload.fromIdx));
    dt.setData("text/plain", String(payload.fromIdx));
  } else {
    dt.setData("application/x-trueshow-media", JSON.stringify(payload));
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
  const fromDt = event.dataTransfer?.getData("application/x-trueshow-song") || null;
  activePayload = null;
  return fromDt || fromStore;
}

export function readDroppedPlaylistIndex(event: DragEvent): number | null {
  const fromStore = activePayload?.type === "playlist-cue" ? activePayload.fromIdx : null;
  const raw = event.dataTransfer?.getData("application/x-trueshow-playlist-idx");
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
  const raw = event.dataTransfer?.getData("application/x-trueshow-media");
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

export function readDroppedArrangementPayload(
  event: DragEvent,
): { kind: "master-group"; group: string } | { kind: "arrangement-slot"; fromIdx: number } | null {
  const storeSnap = activePayload;
  const dtGroup = event.dataTransfer?.getData("application/x-trueshow-master-group") || "";
  const dtIdxRaw = event.dataTransfer?.getData("application/x-trueshow-arr-idx") || "";
  activePayload = null;
  if (dtIdxRaw !== "" && !Number.isNaN(Number(dtIdxRaw))) {
    return { kind: "arrangement-slot", fromIdx: Number(dtIdxRaw) };
  }
  if (storeSnap?.type === "arrangement-slot") {
    return { kind: "arrangement-slot", fromIdx: storeSnap.fromIdx };
  }
  if (dtGroup) {
    return { kind: "master-group", group: dtGroup };
  }
  if (storeSnap?.type === "master-group") {
    return { kind: "master-group", group: storeSnap.group };
  }
  return null;
}
