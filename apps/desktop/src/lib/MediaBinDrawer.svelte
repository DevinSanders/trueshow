<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { allowDrop, clearAppDrag, readDroppedMedia, startAppDrag } from "./dragStore";
  import type { MediaAssetRecord, MediaBinRecord, SceneState } from "./types";

  interface Props {
    onMediaTriggered: (nextState: SceneState) => void;
    onError: (msg: string) => void;
  }

  let { onMediaTriggered, onError }: Props = $props();

  let bins = $state<MediaBinRecord[]>([]);
  let assets = $state<MediaAssetRecord[]>([]);
  let selectedCategory = $state<"all" | "background" | "image" | "video" | "audio">("all");
  let selectedBinId = $state<string>("all");
  let mediaSearch = $state("");

  let showAddBin = $state(false);
  let newBinName = $state("");
  let newBinCategory = $state("background");

  let showAddFile = $state(false);
  let mediaFilePath = $state("");
  let mediaFileKind = $state("auto");
  let mediaHasAlpha = $state(false);
  let dragOverBinId = $state<string | null>(null);

  const visibleAssets = $derived(
    assets.filter((a) => {
      const q = mediaSearch.trim().toLowerCase();
      if (!q) return true;
      return (
        a.file_name.toLowerCase().includes(q) ||
        a.kind.toLowerCase().includes(q) ||
        a.local_path.toLowerCase().includes(q)
      );
    }),
  );

  async function refreshAll() {
    try {
      const [loadedBins, loadedAssets] = await Promise.all([
        invoke<MediaBinRecord[]>("list_media_bins"),
        invoke<MediaAssetRecord[]>("list_media_assets", {
          kind: selectedCategory === "all" ? null : selectedCategory,
          binId: selectedBinId === "all" ? null : selectedBinId,
        }),
      ]);
      bins = loadedBins ?? [];
      assets = loadedAssets ?? [];
    } catch (err) {
      onError(`Failed to load media library: ${String(err)}`);
    }
  }

  async function refreshAssets() {
    try {
      assets =
        (await invoke<MediaAssetRecord[]>("list_media_assets", {
          kind: selectedCategory === "all" ? null : selectedCategory,
          binId: selectedBinId === "all" ? null : selectedBinId,
        })) ?? [];
    } catch (err) {
      onError(`Failed to filter media assets: ${String(err)}`);
    }
  }

  async function addBin() {
    if (!newBinName.trim()) {
      onError("Media bin name cannot be empty.");
      return;
    }
    try {
      bins = await invoke<MediaBinRecord[]>("create_media_bin", {
        name: newBinName.trim(),
        category: newBinCategory,
      });
      newBinName = "";
      showAddBin = false;
    } catch (err) {
      onError(`Create media bin failed: ${String(err)}`);
    }
  }

  async function removeBin(binId: string) {
    try {
      bins = await invoke<MediaBinRecord[]>("delete_media_bin", { binId });
      if (selectedBinId === binId) {
        selectedBinId = "all";
      }
      await refreshAssets();
    } catch (err) {
      onError(`Delete media bin failed: ${String(err)}`);
    }
  }

  async function importLocalMedia() {
    if (!mediaFilePath.trim()) {
      onError("Enter a local file path to import media.");
      return;
    }
    try {
      await invoke<MediaAssetRecord>("add_media_file", {
        filePath: mediaFilePath.trim(),
        binId: selectedBinId === "all" ? null : selectedBinId,
        kind: mediaFileKind,
        hasAlpha: mediaHasAlpha,
      });
      mediaFilePath = "";
      mediaHasAlpha = false;
      showAddFile = false;
      await refreshAssets();
    } catch (err) {
      onError(`Import media failed: ${String(err)}`);
    }
  }

  async function triggerAsset(
    asset: MediaAssetRecord,
    role: "auto" | "background" | "foreground" | "announcement" = "auto",
  ) {
    try {
      const next = await invoke<SceneState>("trigger_media_asset", {
        blake3Hash: asset.blake3_hash,
        role,
      });
      onMediaTriggered(next);
    } catch (err) {
      onError(`Trigger media failed: ${String(err)}`);
    }
  }

  async function moveAssetToBin(blake3Hash: string, targetBinId: string) {
    try {
      await invoke("move_media_to_bin", { blake3Hash, binId: targetBinId });
      await refreshAssets();
    } catch (err) {
      onError(`Move media failed: ${String(err)}`);
    }
  }

  function startMediaDrag(event: DragEvent, asset: MediaAssetRecord) {
    startAppDrag(event, {
      type: "media",
      blake3_hash: asset.blake3_hash,
      kind: asset.kind,
      file_name: asset.file_name,
    });
  }

  function handleDropOnBin(event: DragEvent, targetBinId: string) {
    dragOverBinId = null;
    event.preventDefault();
    const media = readDroppedMedia(event);
    if (media?.blake3_hash) {
      void moveAssetToBin(media.blake3_hash, targetBinId);
    }
  }

  async function removeAsset(asset: MediaAssetRecord) {
    try {
      await invoke("delete_media_asset", { blake3Hash: asset.blake3_hash });
      await refreshAssets();
    } catch (err) {
      onError(`Delete media failed: ${String(err)}`);
    }
  }

  function swatchStyle(asset: MediaAssetRecord): string {
    if (asset.local_path.includes("sanctuary-glow")) {
      return "background: linear-gradient(135deg, #431407, #c2410c);";
    }
    if (asset.local_path.includes("broadcast-grid")) {
      return "background: linear-gradient(135deg, #111827, #1f2937);";
    }
    if (asset.kind === "audio") {
      return "background: linear-gradient(135deg, #312e81, #6d28d9);";
    }
    if (asset.kind === "video") {
      return "background: linear-gradient(135deg, #0f172a, #1d4ed8);";
    }
    if (asset.kind === "image") {
      return "background: linear-gradient(135deg, #064e3b, #0d9488);";
    }
    return "background: linear-gradient(135deg, #0c2340, #0e7490);";
  }

  onMount(() => {
    void refreshAll();
  });
</script>

<section class="media-drawer" data-testid="media-bin-drawer">
  <div class="bins-col">
    <div class="sec-head">
      <h3>Media Bins</h3>
      <button class="mini-btn" onclick={() => (showAddBin = !showAddBin)}>+ Bin</button>
    </div>

    {#if showAddBin}
      <div class="bin-form">
        <input aria-label="New Media Bin Name" placeholder="Bin name..." bind:value={newBinName} />
        <select aria-label="New Media Bin Category" bind:value={newBinCategory}>
          <option value="background">Backgrounds</option>
          <option value="image">Images</option>
          <option value="video">Video</option>
          <option value="audio">Audio</option>
        </select>
        <button class="mini-btn primary" onclick={() => void addBin()}>Add</button>
      </div>
    {/if}

    <ul class="bin-list">
      <li>
        <button
          class="bin-btn"
          class:active={selectedBinId === "all"}
          onclick={() => {
            selectedBinId = "all";
            void refreshAssets();
          }}
        >
          All Bins
        </button>
      </li>
      {#each bins as bin (bin.id)}
        <li
          class="bin-row"
          class:drag-over={dragOverBinId === bin.id}
          ondragenter={(e) => {
            if (allowDrop(e, ["media"], "move")) dragOverBinId = bin.id;
          }}
          ondragover={(e) => {
            if (allowDrop(e, ["media"], "move")) dragOverBinId = bin.id;
          }}
          ondragleave={() => {
            if (dragOverBinId === bin.id) dragOverBinId = null;
          }}
          ondrop={(e) => handleDropOnBin(e, bin.id)}
        >
          <button
            class="bin-btn"
            class:active={selectedBinId === bin.id}
            onclick={() => {
              selectedBinId = bin.id;
              void refreshAssets();
            }}
          >
            {bin.name}
          </button>
          {#if bin.id !== "backgrounds"}
            <button
              class="icon-btn"
              aria-label={`Delete bin ${bin.name}`}
              onclick={() => void removeBin(bin.id)}
            >
              ×
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  </div>

  <div class="assets-col">
    <div class="media-toolbar">
      <div class="cat-tabs" role="tablist" aria-label="Media categories">
        {#each [["all", "All"], ["background", "Backgrounds"], ["image", "Images"], ["video", "Video"], ["audio", "Audio"]] as [val, label] (val)}
          <button
            class="cat-tab"
            class:active={selectedCategory === val}
            onclick={() => {
              selectedCategory = val as typeof selectedCategory;
              void refreshAssets();
            }}
          >
            {label}
          </button>
        {/each}
      </div>

      <div class="media-right-tools">
        <input
          class="media-search"
          aria-label="Filter media assets"
          placeholder="Filter media..."
          bind:value={mediaSearch}
        />
        <button class="mini-btn primary" onclick={() => (showAddFile = !showAddFile)}>
          + Add Media
        </button>
      </div>
    </div>

    {#if showAddFile}
      <div class="import-bar">
        <input
          aria-label="Media File Path"
          placeholder="Local file path (e.g. C:\Media\motion-bg.mp4, logo.png, walkin.mp3)"
          bind:value={mediaFilePath}
        />
        <select aria-label="Media Kind" bind:value={mediaFileKind}>
          <option value="auto">Auto-Detect</option>
          <option value="background">Background</option>
          <option value="image">Image</option>
          <option value="video">Video</option>
          <option value="audio">Audio</option>
        </select>
        <label class="alpha-check">
          <input type="checkbox" bind:checked={mediaHasAlpha} />
          Alpha
        </label>
        <button class="mini-btn primary" onclick={() => void importLocalMedia()}>Import</button>
      </div>
    {/if}

    <div class="asset-grid">
      {#each visibleAssets as asset (asset.blake3_hash)}
        <div
          class="asset-card"
          draggable="true"
          role="group"
          ondragstart={(e) => startMediaDrag(e, asset)}
          ondragend={clearAppDrag}
          title="Click to fire (BG stays behind slides; FG clears on next slide). Drag onto any Slide, Layer, or Bin."
        >
          <button
            class="asset-thumb"
            style={swatchStyle(asset)}
            onclick={() => void triggerAsset(asset, "auto")}
          >
            <span class="kind-badge">
              {asset.kind === "background" ? "BG LAYER" : asset.kind.toUpperCase()}
            </span>
            {#if asset.has_alpha}
              <span class="alpha-badge">ALPHA</span>
            {/if}
            <span class="thumb-name">{asset.file_name}</span>
          </button>
          <div class="asset-foot">
            <div class="role-btns">
              <button
                class="role-pill"
                aria-label={`Set ${asset.file_name} as Background Layer`}
                onclick={() => void triggerAsset(asset, "background")}
              >
                BG
              </button>
              <button
                class="role-pill"
                aria-label={`Show ${asset.file_name} on Foreground Slide Layer`}
                onclick={() => void triggerAsset(asset, "foreground")}
              >
                FG
              </button>
            </div>
            <button
              class="icon-btn danger"
              aria-label={`Remove media ${asset.file_name}`}
              onclick={() => void removeAsset(asset)}
            >
              ×
            </button>
          </div>
        </div>
      {:else}
        <p class="empty-msg">No media assets match this filter. Click + Add Media to import files.</p>
      {/each}
    </div>
  </div>
</section>

<style>
  .media-drawer {
    display: grid;
    grid-template-columns: 185px 1fr;
    border-top: 1px solid var(--border);
    background: var(--bg-1);
    height: 215px;
    min-height: 170px;
    overflow: hidden;
  }

  .bins-col {
    border-right: 1px solid var(--border);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
  }

  .sec-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .sec-head h3 {
    margin: 0;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-1);
  }

  .bin-form {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .bin-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .bin-row {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .bin-btn {
    flex: 1;
    text-align: left;
    background: transparent;
    color: var(--text-0);
    border: 1px solid transparent;
    border-radius: 5px;
    padding: 4px 6px;
    font-size: 11px;
    cursor: pointer;
  }

  .bin-btn.active {
    background: rgba(37, 99, 235, 0.22);
    border-color: rgba(59, 130, 246, 0.5);
    font-weight: 600;
  }

  .assets-col {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    min-width: 0;
    overflow: hidden;
  }

  .media-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .cat-tabs,
  .media-right-tools {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .cat-tab {
    background: var(--bg-2);
    color: var(--text-1);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 8px;
    font-size: 11px;
    cursor: pointer;
  }

  .cat-tab.active {
    background: #1d4ed8;
    border-color: #2563eb;
    color: #fff;
  }

  .media-search {
    width: 150px;
  }

  .import-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
  }

  .import-bar input:not([type="checkbox"]) {
    flex: 1;
  }

  .alpha-check {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--text-1);
  }

  input,
  select {
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 4px 7px;
    font-size: 11px;
  }

  .asset-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 8px;
    overflow-y: auto;
    padding-bottom: 4px;
  }

  .asset-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .asset-thumb {
    position: relative;
    height: 74px;
    border: none;
    color: #fff;
    padding: 6px;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    text-align: left;
    cursor: pointer;
  }

  .kind-badge {
    position: absolute;
    top: 5px;
    left: 5px;
    background: rgba(0, 0, 0, 0.55);
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 9px;
    font-weight: 700;
  }

  .alpha-badge {
    position: absolute;
    top: 5px;
    right: 5px;
    background: #0f766e;
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 9px;
    font-weight: 700;
  }

  .thumb-name {
    font-size: 11px;
    font-weight: 600;
    line-height: 1.2;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.7);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .asset-foot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 3px 6px;
    font-size: 10px;
    color: var(--text-1);
  }

  .mini-btn {
    background: var(--bg-2);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 8px;
    font-size: 11px;
    cursor: pointer;
  }

  .mini-btn.primary {
    background: #1d4ed8;
    border-color: #2563eb;
    color: #fff;
  }

  .icon-btn {
    background: transparent;
    border: none;
    color: var(--text-1);
    cursor: pointer;
    padding: 1px 5px;
    font-size: 12px;
  }

  .icon-btn.danger:hover {
    color: #f87171;
  }

  .empty-msg {
    color: var(--text-1);
    font-size: 12px;
    margin: 8px 0;
  }

  .role-btns {
    display: flex;
    gap: 3px;
  }

  .role-pill {
    background: var(--bg-0);
    color: var(--text-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 9px;
    font-weight: 700;
    cursor: pointer;
  }

  .role-pill:hover {
    color: #fff;
    border-color: #3b82f6;
  }

  .drag-over {
    outline: 1px dashed #38bdf8;
    background: rgba(56, 189, 248, 0.14);
  }
</style>
