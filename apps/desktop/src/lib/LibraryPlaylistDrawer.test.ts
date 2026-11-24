import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import LibraryPlaylistDrawer from "./LibraryPlaylistDrawer.svelte";
import type { PlaylistRecord, PresentationLibraryRecord, SongRecord } from "./types";

describe("LibraryPlaylistDrawer (Presentation Libraries & Playlists)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("manages libraries, creates/loads presentations, and manages/reorders playlists (happy path)", async () => {
    let libs: PresentationLibraryRecord[] = [
      { id: "default", name: "Default Library", updated_at: "2026-11-24T00:00:00Z" },
      { id: "worship", name: "Worship Songs", updated_at: "2026-11-24T00:00:00Z" },
    ];
    let songs: SongRecord[] = [
      {
        id: "song-1",
        library_id: "worship",
        title: "Amazing Grace",
        author: "John Newton",
        copyright: "PD",
        ccli_number: "22025",
        key_signature: "G",
        bpm: 72,
        lyrics_plain: "Amazing grace",
        slides: [],
        updated_at: "2026-11-24T00:00:00Z",
      },
      {
        id: "song-2",
        library_id: "default",
        title: "Psalm 100",
        author: "Scripture",
        copyright: "PD",
        ccli_number: null,
        key_signature: null,
        bpm: null,
        lyrics_plain: "Make a joyful noise",
        slides: [],
        updated_at: "2026-11-24T00:00:00Z",
      },
    ];
    let playlists: PlaylistRecord[] = [
      {
        id: "pl-1",
        name: "Sunday Morning",
        item_song_ids: ["song-1", "song-2"],
        updated_at: "2026-11-24T00:00:00Z",
      },
    ];

    const onDeckLoaded = vi.fn();
    const onError = vi.fn();

    mockIPC((cmd, args) => {
      const payload = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "list_libraries":
          return libs;
        case "create_library":
          libs = [
            ...libs,
            { id: "youth", name: String(payload.name), updated_at: "2026-11-24T00:00:00Z" },
          ];
          return libs;
        case "delete_library":
          libs = libs.filter((l) => l.id !== payload.libraryId);
          return libs;
        case "search_library_songs":
          return songs;
        case "create_presentation": {
          const rec: SongRecord = {
            id: "song-3",
            library_id: String(payload.libraryId),
            title: String(payload.title),
            author: String(payload.author ?? ""),
            copyright: "",
            ccli_number: (payload.ccliNumber as string | null) ?? null,
            key_signature: null,
            bpm: null,
            lyrics_plain: String(payload.bodyText),
            slides: [],
            updated_at: "2026-11-24T00:00:00Z",
          };
          songs = [...songs, rec];
          return rec;
        }
        case "delete_presentation":
          songs = songs.filter((s) => s.id !== payload.songId);
          return true;
        case "load_presentation_to_deck":
          return { revision: 2, deck: [] };
        case "list_playlists":
          return playlists;
        case "upsert_playlist": {
          const id = (payload.id as string | null) ?? `pl-${playlists.length + 1}`;
          const rec: PlaylistRecord = {
            id,
            name: String(payload.name),
            item_song_ids: (payload.itemSongIds as string[]) ?? [],
            updated_at: "2026-11-24T00:00:00Z",
          };
          const exists = playlists.some((p) => p.id === id);
          playlists = exists ? playlists.map((p) => (p.id === id ? rec : p)) : [rec, ...playlists];
          return playlists;
        }
        case "delete_playlist":
          playlists = playlists.filter((p) => p.id !== payload.playlistId);
          return playlists;
        default:
          return null;
      }
    });

    render(LibraryPlaylistDrawer, { props: { onDeckLoaded, onError } });

    await waitFor(() => expect(screen.getByText("Amazing Grace")).toBeTruthy());

    // 1. Create and filter a new library
    await fireEvent.click(screen.getByText("+ Library"));
    await fireEvent.input(screen.getByLabelText("New Library Name"), {
      target: { value: "Youth Ministry" },
    });
    await fireEvent.click(screen.getByText("Add"));
    await waitFor(() => expect(screen.getByText("Youth Ministry")).toBeTruthy());
    await fireEvent.click(screen.getByText("Youth Ministry"));
    await fireEvent.click(screen.getByLabelText("Delete library Youth Ministry"));

    // 2. Create a new presentation & open it in the live Cue Deck
    await fireEvent.click(screen.getByText("+ New"));
    await fireEvent.input(screen.getByLabelText("Presentation Title"), {
      target: { value: "Call to Worship" },
    });
    await fireEvent.input(screen.getByLabelText("Presentation Slides"), {
      target: { value: "Slide 1\n\nSlide 2" },
    });
    await fireEvent.click(screen.getByText("Create Presentation"));
    await waitFor(() => expect(screen.getByText("Call to Worship")).toBeTruthy());
    await fireEvent.click(screen.getByText("Call to Worship"));
    await waitFor(() => expect(onDeckLoaded).toHaveBeenCalledTimes(1));

    // 3. Add presentation to playlist, reorder down/up, remove item, create & delete playlist
    await fireEvent.click(screen.getByLabelText("Add Call to Worship to playlist"));
    await fireEvent.click(screen.getByLabelText("Move item 1 down"));
    await fireEvent.click(screen.getByLabelText("Move item 2 up"));
    await fireEvent.click(screen.getByLabelText("Remove item 1 from playlist"));

    await fireEvent.click(screen.getByText("+ Playlist"));
    await fireEvent.input(screen.getByLabelText("New Playlist Name"), {
      target: { value: "Wednesday Night" },
    });
    await fireEvent.click(screen.getByText("Add"));
    await waitFor(() => expect(screen.getByText(/Wednesday Night/)).toBeTruthy());
    await fireEvent.click(screen.getByLabelText("Delete playlist Wednesday Night"));

    await fireEvent.click(screen.getByLabelText("Delete Call to Worship"));
    expect(onError).not.toHaveBeenCalled();
  });

  it("reports validation and IPC failures cleanly via onError", async () => {
    const onDeckLoaded = vi.fn();
    const onError = vi.fn();

    mockIPC((cmd) => {
      if (cmd === "list_libraries") throw new Error("db locked");
      if (cmd === "create_library") throw new Error("duplicate lib");
      if (cmd === "create_presentation") throw new Error("disk full");
      if (cmd === "upsert_playlist") throw new Error("playlist write error");
      return [];
    });

    render(LibraryPlaylistDrawer, { props: { onDeckLoaded, onError } });
    await waitFor(() => expect(onError).toHaveBeenCalledWith(expect.stringContaining("db locked")));

    // Validation failures for blank names
    await fireEvent.click(screen.getByText("+ Library"));
    await fireEvent.click(screen.getByText("Add"));
    expect(onError).toHaveBeenCalledWith("Library name cannot be empty.");

    await fireEvent.click(screen.getByText("+ New"));
    await fireEvent.click(screen.getByText("Create Presentation"));
    expect(onError).toHaveBeenCalledWith("Presentation title cannot be empty.");

    await fireEvent.click(screen.getByText("+ Playlist"));
    await fireEvent.click(screen.getAllByText("Add")[1]);
    expect(onError).toHaveBeenCalledWith("Playlist name cannot be empty.");
  });
});
