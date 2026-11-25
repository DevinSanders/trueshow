import { describe, expect, it, vi } from "vitest";
import {
  allowDrop,
  clearAppDrag,
  readDroppedMedia,
  readDroppedPlaylistIndex,
  readDroppedSongId,
  startAppDrag,
} from "./dragStore";

function fakeDragEvent(data: Record<string, string> = {}): DragEvent {
  const store: Record<string, string> = { ...data };
  return {
    preventDefault: vi.fn(),
    dataTransfer: {
      effectAllowed: "none",
      dropEffect: "none",
      setData: (k: string, v: string) => {
        store[k] = v;
      },
      getData: (k: string) => store[k] ?? "",
    },
  } as unknown as DragEvent;
}

describe("dragStore WebView2-safe drag-and-drop state", () => {
  it("supports song, playlist-cue, and media drags including store fallback and type filtering", () => {
    // 1. Song drag: accepted by song targets, rejected by media-only targets
    const songEv = fakeDragEvent();
    startAppDrag(songEv, { type: "song", songId: "song-42", title: "Amazing Grace" });
    expect(allowDrop(fakeDragEvent(), ["media"], "copy")).toBe(false);
    const acceptEv = fakeDragEvent();
    expect(allowDrop(acceptEv, ["song"], "move")).toBe(true);
    expect(acceptEv.preventDefault).toHaveBeenCalled();
    // Fallback to in-memory store even when dataTransfer is empty
    expect(readDroppedSongId(fakeDragEvent())).toBe("song-42");

    // 2. Playlist cue drag
    startAppDrag(fakeDragEvent(), { type: "playlist-cue", fromIdx: 3 });
    expect(allowDrop(fakeDragEvent(), ["playlist-cue"], "move")).toBe(true);
    expect(readDroppedPlaylistIndex(fakeDragEvent())).toBe(3);

    // 3. Media drag with store fallback + JSON dataTransfer + malformed JSON safety
    startAppDrag(fakeDragEvent(), {
      type: "media",
      blake3_hash: "h-123",
      kind: "background",
      file_name: "Aurora.mp4",
    });
    expect(readDroppedMedia(fakeDragEvent())).toEqual({
      blake3_hash: "h-123",
      kind: "background",
      file_name: "Aurora.mp4",
    });
    expect(
      readDroppedMedia(fakeDragEvent({ "application/x-trueshow-media": "{bad" })),
    ).toBeNull();

    // 4. clearAppDrag resets active drag cleanly
    startAppDrag(fakeDragEvent(), { type: "song", songId: "s-9", title: "Temp" });
    clearAppDrag();
    expect(readDroppedSongId(fakeDragEvent())).toBeNull();
  });
});
