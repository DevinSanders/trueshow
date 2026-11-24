<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import {
    allowDrop,
    clearAppDrag,
    readDroppedPlaylistIndex,
    readDroppedSongId,
    startAppDrag,
  } from "./dragStore";
  import { openContextMenu, type ContextMenuItem } from "./contextMenuStore.svelte";
  import type {
    PlaylistRecord,
    PresentationLibraryRecord,
    SceneState,
    SongRecord,
  } from "./types";

  interface Props {
    onDeckLoaded: (nextState: SceneState) => void;
    onError: (msg: string) => void;
  }

  let { onDeckLoaded, onError }: Props = $props();

  let libraries = $state<PresentationLibraryRecord[]>([]);
  let selectedLibraryId = $state<string>("all");
  let songs = $state<SongRecord[]>([]);
  let searchQuery = $state("");

  let playlists = $state<PlaylistRecord[]>([]);
  let selectedPlaylistId = $state<string | null>(null);

  let showNewLib = $state(false);
  let newLibName = $state("");

  let showNewPres = $state(false);
  let newPresTitle = $state("");
  let newPresAuthor = $state("");
  let newPresCcli = $state("");
  let newPresBody = $state("");

  let showNewPlaylist = $state(false);
  let newPlaylistName = $state("");

  let dragOverLibraryId = $state<string | null>(null);
  let dragOverPlaylistId = $state<string | null>(null);
  let dragOverPlaylistList = $state(false);
  let dragOverCueIdx = $state<number | null>(null);

  const activePlaylist = $derived(
    playlists.find((p) => p.id === selectedPlaylistId) ?? playlists[0] ?? null,
  );

  const songById = $derived(new Map(songs.map((s) => [s.id, s])));

  export async function refreshAll() {
    try {
      const [libs, pls, foundSongs] = await Promise.all([
        invoke<PresentationLibraryRecord[]>("list_libraries"),
        invoke<PlaylistRecord[]>("list_playlists"),
        invoke<SongRecord[]>("search_library_songs", {
          query: searchQuery,
          libraryId: selectedLibraryId,
        }),
      ]);
      libraries = libs ?? [];
      playlists = pls ?? [];
      songs = foundSongs ?? [];
      if (!selectedPlaylistId && playlists.length > 0) {
        selectedPlaylistId = playlists[0].id;
      }
    } catch (err) {
      onError(`Library load failed: ${String(err)}`);
    }
  }

  async function refreshSongs() {
    try {
      songs =
        (await invoke<SongRecord[]>("search_library_songs", {
          query: searchQuery,
          libraryId: selectedLibraryId,
        })) ?? [];
    } catch (err) {
      onError(`Library search failed: ${String(err)}`);
    }
  }

  async function addLibrary() {
    if (!newLibName.trim()) {
      onError("Library name cannot be empty.");
      return;
    }
    try {
      libraries = await invoke<PresentationLibraryRecord[]>("create_library", {
        name: newLibName.trim(),
      });
      newLibName = "";
      showNewLib = false;
    } catch (err) {
      onError(`Create library failed: ${String(err)}`);
    }
  }

  async function removeLibrary(id: string) {
    try {
      libraries = await invoke<PresentationLibraryRecord[]>("delete_library", {
        libraryId: id,
      });
      if (selectedLibraryId === id) {
        selectedLibraryId = "all";
      }
      await refreshSongs();
    } catch (err) {
      onError(`Delete library failed: ${String(err)}`);
    }
  }

  async function addPresentation() {
    if (!newPresTitle.trim()) {
      onError("Presentation title cannot be empty.");
      return;
    }
    try {
      await invoke<SongRecord>("create_presentation", {
        libraryId: selectedLibraryId === "all" ? "default" : selectedLibraryId,
        title: newPresTitle.trim(),
        bodyText: newPresBody,
        author: newPresAuthor.trim() || null,
        ccliNumber: newPresCcli.trim() || null,
      });
      newPresTitle = "";
      newPresAuthor = "";
      newPresCcli = "";
      newPresBody = "";
      showNewPres = false;
      await refreshSongs();
    } catch (err) {
      onError(`Create presentation failed: ${String(err)}`);
    }
  }

  async function removePresentation(songId: string) {
    try {
      await invoke("delete_presentation", { songId });
      await refreshSongs();
    } catch (err) {
      onError(`Delete presentation failed: ${String(err)}`);
    }
  }

  async function openPresentation(songId: string) {
    try {
      const next = await invoke<SceneState>("load_presentation_to_deck", { songId });
      onDeckLoaded(next);
    } catch (err) {
      onError(`Failed to load presentation: ${String(err)}`);
    }
  }

  async function addPlaylist() {
    if (!newPlaylistName.trim()) {
      onError("Playlist name cannot be empty.");
      return;
    }
    try {
      const updated = await invoke<PlaylistRecord[]>("upsert_playlist", {
        id: null,
        name: newPlaylistName.trim(),
        itemSongIds: [],
      });
      playlists = updated;
      if (updated.length > 0) {
        selectedPlaylistId = updated[0].id;
      }
      newPlaylistName = "";
      showNewPlaylist = false;
    } catch (err) {
      onError(`Create playlist failed: ${String(err)}`);
    }
  }

  async function removePlaylist(id: string) {
    try {
      playlists = await invoke<PlaylistRecord[]>("delete_playlist", { playlistId: id });
      if (selectedPlaylistId === id) {
        selectedPlaylistId = playlists[0]?.id ?? null;
      }
    } catch (err) {
      onError(`Delete playlist failed: ${String(err)}`);
    }
  }

  async function addSongToActivePlaylist(songId: string) {
    if (!activePlaylist) {
      onError("Create or select a playlist first.");
      return;
    }
    try {
      playlists = await invoke<PlaylistRecord[]>("upsert_playlist", {
        id: activePlaylist.id,
        name: activePlaylist.name,
        itemSongIds: [...activePlaylist.item_song_ids, songId],
      });
    } catch (err) {
      onError(`Failed to add to playlist: ${String(err)}`);
    }
  }

  async function movePlaylistItem(index: number, delta: number) {
    if (!activePlaylist) return;
    const target = index + delta;
    if (target < 0 || target >= activePlaylist.item_song_ids.length) return;
    const nextIds = [...activePlaylist.item_song_ids];
    const [moved] = nextIds.splice(index, 1);
    nextIds.splice(target, 0, moved);
    try {
      playlists = await invoke<PlaylistRecord[]>("upsert_playlist", {
        id: activePlaylist.id,
        name: activePlaylist.name,
        itemSongIds: nextIds,
      });
    } catch (err) {
      onError(`Failed to reorder playlist: ${String(err)}`);
    }
  }

  async function removePlaylistItem(index: number) {
    if (!activePlaylist) return;
    const nextIds = activePlaylist.item_song_ids.filter((_, i) => i !== index);
    try {
      playlists = await invoke<PlaylistRecord[]>("upsert_playlist", {
        id: activePlaylist.id,
        name: activePlaylist.name,
        itemSongIds: nextIds,
      });
    } catch (err) {
      onError(`Failed to remove playlist item: ${String(err)}`);
    }
  }

  async function moveSongToLibrary(songId: string, targetLibId: string) {
    try {
      await invoke("move_presentation_to_library", { songId, libraryId: targetLibId });
      await refreshSongs();
    } catch (err) {
      onError(`Move presentation failed: ${String(err)}`);
    }
  }

  async function addSongToPlaylistById(playlistId: string, songId: string) {
    const pl = playlists.find((p) => p.id === playlistId);
    if (!pl) return;
    try {
      playlists = await invoke<PlaylistRecord[]>("upsert_playlist", {
        id: pl.id,
        name: pl.name,
        itemSongIds: [...pl.item_song_ids, songId],
      });
    } catch (err) {
      onError(`Failed to add to playlist: ${String(err)}`);
    }
  }

  async function reorderPlaylistDrag(fromIdx: number, toIdx: number) {
    if (!activePlaylist || fromIdx === toIdx) return;
    if (fromIdx < 0 || toIdx < 0 || fromIdx >= activePlaylist.item_song_ids.length) return;
    const nextIds = [...activePlaylist.item_song_ids];
    const [moved] = nextIds.splice(fromIdx, 1);
    nextIds.splice(toIdx, 0, moved);
    try {
      playlists = await invoke<PlaylistRecord[]>("upsert_playlist", {
        id: activePlaylist.id,
        name: activePlaylist.name,
        itemSongIds: nextIds,
      });
    } catch (err) {
      onError(`Failed to reorder playlist: ${String(err)}`);
    }
  }

  function startSongDrag(event: DragEvent, song: SongRecord) {
    startAppDrag(event, { type: "song", songId: song.id, title: song.title });
  }

  function openSongContextMenu(event: MouseEvent, song: SongRecord) {
    const items: ContextMenuItem[] = [{ id: "open", label: "Load into Cue Deck" }];
    if (activePlaylist) {
      items.push({ id: "add-pl", label: `Add to "${activePlaylist.name}"` });
    }
    libraries
      .filter((l) => l.id !== song.library_id)
      .forEach((lib, i) => {
        items.push({ id: `move:${lib.id}`, label: `Move to Library: ${lib.name}`, separator: i === 0 });
      });
    items.push({ id: "delete", label: "Delete Presentation", danger: true, separator: true });
    openContextMenu(
      event,
      items,
      (id) => {
        if (id === "open") void openPresentation(song.id);
        else if (id === "add-pl") void addSongToActivePlaylist(song.id);
        else if (id.startsWith("move:")) void moveSongToLibrary(song.id, id.slice(5));
        else if (id === "delete") void removePresentation(song.id);
      },
      song.title,
    );
  }

  function openCueContextMenu(event: MouseEvent, songId: string, idx: number, title: string) {
    const len = activePlaylist?.item_song_ids.length ?? 0;
    const items: ContextMenuItem[] = [
      { id: "open", label: "Load into Cue Deck" },
      { id: "up", label: "Move Up", disabled: idx <= 0, separator: true },
      { id: "down", label: "Move Down", disabled: idx >= len - 1 },
      { id: "remove", label: "Remove from Playlist", danger: true, separator: true },
    ];
    openContextMenu(
      event,
      items,
      (id) => {
        if (id === "open") void openPresentation(songId);
        else if (id === "up") void movePlaylistItem(idx, -1);
        else if (id === "down") void movePlaylistItem(idx, 1);
        else if (id === "remove") void removePlaylistItem(idx);
      },
      title,
    );
  }

  function handleDropOnLibrary(event: DragEvent, targetLibId: string) {
    dragOverLibraryId = null;
    event.preventDefault();
    const songId = readDroppedSongId(event);
    if (songId) {
      void moveSongToLibrary(songId, targetLibId);
    }
  }

  function handleDropOnPlaylist(event: DragEvent, playlistId: string) {
    dragOverPlaylistId = null;
    dragOverPlaylistList = false;
    event.preventDefault();
    const songId = readDroppedSongId(event);
    if (songId) {
      void addSongToPlaylistById(playlistId, songId);
    }
  }

  function handleDropOnCueRow(event: DragEvent, targetIdx: number) {
    dragOverCueIdx = null;
    event.preventDefault();
    const fromIdx = readDroppedPlaylistIndex(event);
    if (fromIdx !== null) {
      void reorderPlaylistDrag(fromIdx, targetIdx);
      return;
    }
    const songId = readDroppedSongId(event);
    if (songId && activePlaylist) {
      void addSongToPlaylistById(activePlaylist.id, songId);
    }
  }

  onMount(() => {
    void refreshAll();
  });
</script>

<aside class="drawer" data-testid="library-playlist-drawer">
  <section class="drawer-section">
    <div class="sec-head">
      <h2>Libraries</h2>
      <button class="mini-btn" onclick={() => (showNewLib = !showNewLib)}>+ Library</button>
    </div>

    {#if showNewLib}
      <div class="inline-form">
        <input
          aria-label="New Library Name"
          placeholder="e.g. Kids Ministry"
          bind:value={newLibName}
        />
        <button class="mini-btn primary" onclick={() => void addLibrary()}>Add</button>
      </div>
    {/if}

    <ul class="folder-list">
      <li>
        <button
          class="folder-btn"
          class:selected={selectedLibraryId === "all"}
          onclick={() => {
            selectedLibraryId = "all";
            void refreshSongs();
          }}
        >
          All Libraries
        </button>
      </li>
      {#each libraries as lib (lib.id)}
        <li
          class="folder-row"
          class:drag-over={dragOverLibraryId === lib.id}
          ondragenter={(e) => {
            if (allowDrop(e, ["song"], "move")) dragOverLibraryId = lib.id;
          }}
          ondragover={(e) => {
            if (allowDrop(e, ["song"], "move")) dragOverLibraryId = lib.id;
          }}
          ondragleave={() => {
            if (dragOverLibraryId === lib.id) dragOverLibraryId = null;
          }}
          ondrop={(e) => handleDropOnLibrary(e, lib.id)}
        >
          <button
            class="folder-btn"
            class:selected={selectedLibraryId === lib.id}
            onclick={() => {
              selectedLibraryId = lib.id;
              void refreshSongs();
            }}
          >
            {lib.name}
          </button>
          {#if lib.id !== "default"}
            <button
              class="icon-btn"
              title={`Delete library ${lib.name}`}
              aria-label={`Delete library ${lib.name}`}
              onclick={() => void removeLibrary(lib.id)}
            >
              ×
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>

  <section class="drawer-section grow">
    <div class="sec-head">
      <h2>Presentations</h2>
      <button class="mini-btn" onclick={() => (showNewPres = !showNewPres)}>+ New</button>
    </div>

    <input
      class="search-input"
      aria-label="Search songs"
      placeholder="Search lyrics, title, or CCLI #..."
      bind:value={searchQuery}
      oninput={() => void refreshSongs()}
    />

    {#if showNewPres}
      <div class="stack-form">
        <input aria-label="Presentation Title" placeholder="Title" bind:value={newPresTitle} />
        <div class="row-2">
          <input aria-label="Presentation Author" placeholder="Author" bind:value={newPresAuthor} />
          <input aria-label="Presentation CCLI" placeholder="CCLI #" bind:value={newPresCcli} />
        </div>
        <textarea
          aria-label="Presentation Slides"
          rows="3"
          placeholder="Slide 1 text&#10;&#10;Slide 2 text (blank line splits slides)"
          bind:value={newPresBody}
        ></textarea>
        <button class="mini-btn primary" onclick={() => void addPresentation()}>
          Create Presentation
        </button>
      </div>
    {/if}

    <ul class="item-list">
      {#each songs as song (song.id)}
        <li
          class="item-row"
          draggable="true"
          ondragstart={(e) => startSongDrag(e, song)}
          ondragend={clearAppDrag}
          oncontextmenu={(e) => openSongContextMenu(e, song)}
          title="Drag onto a library to move, or onto a playlist to add"
        >
          <button class="item-main" onclick={() => void openPresentation(song.id)}>
            <strong>{song.title}</strong>
            <span class="meta">
              {song.author || "Presentation"}{song.ccli_number ? ` · CCLI #${song.ccli_number}` : ""}
            </span>
          </button>
          <div class="item-actions">
            <button
              class="icon-btn"
              title="Add to active playlist"
              aria-label={`Add ${song.title} to playlist`}
              onclick={() => void addSongToActivePlaylist(song.id)}
            >
              +
            </button>
            <button
              class="icon-btn danger"
              title={`Delete ${song.title}`}
              aria-label={`Delete ${song.title}`}
              onclick={() => void removePresentation(song.id)}
            >
              ×
            </button>
          </div>
        </li>
      {:else}
        <li class="muted-li">No matching presentations in SQLite library</li>
      {/each}
    </ul>
  </section>

  <section class="drawer-section playlist-sec">
    <div class="sec-head">
      <h2>Playlists</h2>
      <button class="mini-btn" onclick={() => (showNewPlaylist = !showNewPlaylist)}>
        + Playlist
      </button>
    </div>

    {#if showNewPlaylist}
      <div class="inline-form">
        <input
          aria-label="New Playlist Name"
          placeholder="e.g. Sunday 10:30 AM"
          bind:value={newPlaylistName}
        />
        <button class="mini-btn primary" onclick={() => void addPlaylist()}>Add</button>
      </div>
    {/if}

    <div class="playlist-tabs">
      {#each playlists as pl (pl.id)}
        <div
          class="pl-chip"
          class:active={activePlaylist?.id === pl.id}
          class:drag-over={dragOverPlaylistId === pl.id}
          role="group"
          ondragenter={(e) => {
            if (allowDrop(e, ["song"], "copy")) dragOverPlaylistId = pl.id;
          }}
          ondragover={(e) => {
            if (allowDrop(e, ["song"], "copy")) dragOverPlaylistId = pl.id;
          }}
          ondragleave={() => {
            if (dragOverPlaylistId === pl.id) dragOverPlaylistId = null;
          }}
          ondrop={(e) => handleDropOnPlaylist(e, pl.id)}
        >
          <button class="pl-select" onclick={() => (selectedPlaylistId = pl.id)}>
            {pl.name} ({pl.item_song_ids.length})
          </button>
          <button
            class="icon-btn"
            aria-label={`Delete playlist ${pl.name}`}
            onclick={() => void removePlaylist(pl.id)}
          >
            ×
          </button>
        </div>
      {:else}
        <span class="muted-li">No playlists yet</span>
      {/each}
    </div>

    {#if activePlaylist}
      <ul
        class="item-list compact"
        class:drag-over={dragOverPlaylistList}
        ondragenter={(e) => {
          if (allowDrop(e, ["song", "playlist-cue"], "copy")) dragOverPlaylistList = true;
        }}
        ondragover={(e) => {
          if (allowDrop(e, ["song", "playlist-cue"], "copy")) dragOverPlaylistList = true;
        }}
        ondragleave={() => {
          dragOverPlaylistList = false;
        }}
        ondrop={(e) => handleDropOnPlaylist(e, activePlaylist.id)}
      >
        {#each activePlaylist.item_song_ids as songId, idx (`${songId}-${idx}`)}
          {@const song = songById.get(songId)}
          <li
            class="item-row"
            class:drag-over={dragOverCueIdx === idx}
            draggable="true"
            ondragstart={(e) => startAppDrag(e, { type: "playlist-cue", fromIdx: idx })}
            ondragend={clearAppDrag}
            ondragenter={(e) => {
              e.stopPropagation();
              if (allowDrop(e, ["playlist-cue", "song"], "move")) dragOverCueIdx = idx;
            }}
            ondragover={(e) => {
              e.stopPropagation();
              if (allowDrop(e, ["playlist-cue", "song"], "move")) dragOverCueIdx = idx;
            }}
            ondragleave={() => {
              if (dragOverCueIdx === idx) dragOverCueIdx = null;
            }}
            ondrop={(e) => {
              e.stopPropagation();
              handleDropOnCueRow(e, idx);
            }}
            oncontextmenu={(e) => openCueContextMenu(e, songId, idx, song?.title ?? songId)}
          >
            <button class="item-main" onclick={() => void openPresentation(songId)}>
              <strong>{idx + 1}. {song ? song.title : songId.slice(0, 8)}</strong>
            </button>
            <div class="item-actions">
              <button
                class="icon-btn"
                aria-label={`Move item ${idx + 1} up`}
                disabled={idx === 0}
                onclick={() => void movePlaylistItem(idx, -1)}
              >
                ↑
              </button>
              <button
                class="icon-btn"
                aria-label={`Move item ${idx + 1} down`}
                disabled={idx === activePlaylist.item_song_ids.length - 1}
                onclick={() => void movePlaylistItem(idx, 1)}
              >
                ↓
              </button>
              <button
                class="icon-btn danger"
                aria-label={`Remove item ${idx + 1} from playlist`}
                onclick={() => void removePlaylistItem(idx)}
              >
                ×
              </button>
            </div>
          </li>
        {:else}
          <li class="muted-li">Click + on any presentation to add to playlist</li>
        {/each}
      </ul>
    {/if}
  </section>
</aside>

<style>
  .drawer {
    display: flex;
    flex-direction: column;
    background: var(--bg-1);
    border-right: 1px solid var(--border);
    min-height: 0;
    overflow: hidden;
  }

  .drawer-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    min-height: 0;
  }

  .drawer-section.grow {
    flex: 1 1 42%;
    overflow: hidden;
  }

  .drawer-section.playlist-sec {
    flex: 1 1 36%;
    border-bottom: none;
    overflow: hidden;
  }

  .sec-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
  }

  h2 {
    margin: 0;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-1);
  }

  .inline-form {
    display: flex;
    gap: 4px;
  }

  .stack-form {
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: var(--bg-2);
    padding: 6px;
    border-radius: 6px;
    border: 1px solid var(--border);
  }

  .row-2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
  }

  input,
  textarea {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 5px 7px;
    font-size: 11px;
  }

  .folder-list,
  .item-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
    overflow-y: auto;
  }

  .folder-list {
    max-height: 110px;
  }

  .folder-row,
  .item-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .folder-btn {
    flex: 1;
    text-align: left;
    background: transparent;
    color: var(--text-0);
    border: 1px solid transparent;
    border-radius: 5px;
    padding: 4px 7px;
    font-size: 12px;
    cursor: pointer;
  }

  .folder-btn.selected {
    background: rgba(37, 99, 235, 0.22);
    border-color: rgba(59, 130, 246, 0.5);
    font-weight: 600;
  }

  .item-row {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 6px;
  }

  .item-main {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    color: var(--text-0);
    text-align: left;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 2px;
    font-size: 12px;
  }

  .item-main strong {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta {
    font-size: 10px;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-actions {
    display: flex;
    gap: 2px;
  }

  .playlist-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .pl-chip {
    display: flex;
    align-items: center;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 5px;
    overflow: hidden;
  }

  .pl-chip.active {
    border-color: #3b82f6;
    background: rgba(37, 99, 235, 0.2);
  }

  .pl-select {
    background: transparent;
    border: none;
    color: var(--text-0);
    font-size: 11px;
    padding: 3px 6px;
    cursor: pointer;
  }

  .mini-btn {
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 7px;
    font-size: 11px;
    cursor: pointer;
    white-space: nowrap;
  }

  .mini-btn.primary {
    background: #1d4ed8;
    border-color: #2563eb;
    color: #fff;
  }

  .icon-btn {
    background: transparent;
    color: var(--text-1);
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 12px;
    cursor: pointer;
  }

  .icon-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-0);
  }

  .icon-btn.danger:hover {
    color: #f87171;
  }

  .icon-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .muted-li {
    font-size: 11px;
    color: var(--text-1);
    padding: 4px 2px;
  }

  .drag-over {
    outline: 1px dashed #38bdf8;
    background: rgba(56, 189, 248, 0.14) !important;
  }
</style>
