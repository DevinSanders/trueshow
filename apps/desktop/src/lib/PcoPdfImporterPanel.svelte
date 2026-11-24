<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { MediaBinRecord, PresentationLibraryRecord, SceneState } from "./types";

  interface Props {
    onSceneChange?: (next: SceneState) => void;
  }

  let { onSceneChange }: Props = $props();

  interface PcoPlanItem {
    id: string;
    title: string;
    item_type: string;
    song_ccli: string | null;
    arrangement_key: string | null;
    description: string | null;
  }

  interface PcoPlanSummary {
    id: string;
    title: string;
    dates: string;
    items: PcoPlanItem[];
  }

  interface PcoAttachment {
    id: string;
    filename: string;
    display_name: string;
    content_type: string | null;
    file_size: number | null;
    kind: "LyricPdf" | "Image" | "Video" | "Audio" | "Other";
    context_label: string;
    open_endpoint: string;
    download_url: string | null;
  }

  interface PdfImportResult {
    song_id: string;
    library_id: string;
    title: string;
    author: string;
    ccli_number: string | null;
    slide_count: number;
    arrangement_count: number;
    scene: SceneState | null;
  }

  interface SafeMediaImportResult {
    asset: { blake3_hash: string; bin_id: string; file_name: string; local_path: string; kind: string };
    safe_file_name: string;
    deduplicated: boolean;
  }

  interface PcoAttachmentImportResult {
    kind: string;
    summary: string;
    scene: SceneState | null;
  }

  let libraries = $state<PresentationLibraryRecord[]>([]);
  let mediaBins = $state<MediaBinRecord[]>([]);
  let targetLibraryId = $state("worship-songs");
  let targetMediaBinId = $state("backgrounds");

  let pdfFilePath = $state("");
  let loadPdfToDeck = $state(true);

  let mediaFilePath = $state("");
  let mediaContextLabel = $state("Sunday Service");

  let pcoAppId = $state("");
  let pcoSecret = $state("");
  let pcoServiceTypeId = $state("");
  let pcoPlans = $state<PcoPlanSummary[]>([]);
  let selectedPlanId = $state<string | null>(null);
  let pcoAttachments = $state<PcoAttachment[]>([]);

  let busy = $state(false);
  let statusMsg = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  async function loadTargets() {
    try {
      const [libs, bins] = await Promise.all([
        invoke<PresentationLibraryRecord[]>("list_libraries").catch(() => []),
        invoke<MediaBinRecord[]>("list_media_bins").catch(() => []),
      ]);
      if (Array.isArray(libs) && libs.length > 0) {
        libraries = libs;
        if (!libs.some((l) => l.id === targetLibraryId)) {
          targetLibraryId = libs[0].id;
        }
      }
      if (Array.isArray(bins) && bins.length > 0) {
        mediaBins = bins;
        if (!bins.some((b) => b.id === targetMediaBinId)) {
          targetMediaBinId = bins[0].id;
        }
      }
    } catch {
      // ignore offline mount errors
    }
  }

  async function importLocalFile() {
    errorMsg = null;
    statusMsg = null;
    if (!pdfFilePath.trim()) {
      errorMsg = "Enter a local file or folder path (.pdf, .pptx, .key, images, .show, .pro6, .cho)";
      return;
    }
    busy = true;
    try {
      const res = await invoke<PdfImportResult>("import_local_presentation_file", {
        filePath: pdfFilePath.trim(),
        libraryId: targetLibraryId,
        loadToDeck: loadPdfToDeck,
      });
      if (res.scene) onSceneChange?.(res.scene);
      const ccliText = res.ccli_number ? `, CCLI #${res.ccli_number}` : "";
      statusMsg = `Imported "${res.title}" (${res.slide_count} slides${ccliText}) into library "${res.library_id}"`;
    } catch (err) {
      errorMsg = String(err);
    } finally {
      busy = false;
    }
  }

  async function importLocalMediaSafely() {
    errorMsg = null;
    statusMsg = null;
    if (!mediaFilePath.trim()) {
      errorMsg = "Enter a local media file path to import safely";
      return;
    }
    busy = true;
    try {
      const res = await invoke<SafeMediaImportResult>("import_media_file_safe", {
        filePath: mediaFilePath.trim(),
        contextLabel: mediaContextLabel.trim() || "Presentation",
        targetBinId: targetMediaBinId,
      });
      statusMsg = `Imported "${res.asset.file_name}" → ${res.safe_file_name} into bin "${res.asset.bin_id}"${res.deduplicated ? " (deduplicated)" : ""}`;
    } catch (err) {
      errorMsg = String(err);
    } finally {
      busy = false;
    }
  }

  async function fetchPcoPlans() {
    errorMsg = null;
    statusMsg = null;
    if (!pcoAppId.trim() || !pcoServiceTypeId.trim()) {
      errorMsg = "Enter your Planning Center App ID / Bearer Token and Service Type ID";
      return;
    }
    busy = true;
    try {
      pcoPlans = await invoke<PcoPlanSummary[]>("pco_list_plans", {
        appIdOrBearer: pcoAppId.trim(),
        secret: pcoSecret.trim() || null,
        serviceTypeId: pcoServiceTypeId.trim(),
      });
      pcoAttachments = [];
      selectedPlanId = null;
      statusMsg = `Loaded ${pcoPlans.length} upcoming Planning Center plan(s).`;
    } catch (err) {
      errorMsg = String(err);
    } finally {
      busy = false;
    }
  }

  async function loadPlanAttachments(plan: PcoPlanSummary) {
    errorMsg = null;
    statusMsg = null;
    busy = true;
    try {
      selectedPlanId = plan.id;
      pcoAttachments = await invoke<PcoAttachment[]>("pco_list_plan_attachments", {
        appIdOrBearer: pcoAppId.trim(),
        secret: pcoSecret.trim() || null,
        serviceTypeId: pcoServiceTypeId.trim(),
        planId: plan.id,
        planTitle: `${plan.title}${plan.dates ? ` (${plan.dates})` : ""}`,
      });
      statusMsg = `Found ${pcoAttachments.length} attachment(s) on "${plan.title}".`;
    } catch (err) {
      errorMsg = String(err);
    } finally {
      busy = false;
    }
  }

  async function importAttachment(att: PcoAttachment) {
    errorMsg = null;
    statusMsg = null;
    busy = true;
    try {
      const res = await invoke<PcoAttachmentImportResult>("pco_import_attachment", {
        appIdOrBearer: pcoAppId.trim(),
        secret: pcoSecret.trim() || null,
        attachment: att,
        targetLibraryId,
        targetMediaBinId,
        loadToDeck: att.kind === "LyricPdf" && loadPdfToDeck,
      });
      if (res.scene) onSceneChange?.(res.scene);
      statusMsg = res.summary;
    } catch (err) {
      errorMsg = String(err);
    } finally {
      busy = false;
    }
  }

  async function importAllSupportedAttachments() {
    const supported = pcoAttachments.filter((a) => a.kind !== "Other");
    if (supported.length === 0) return;
    errorMsg = null;
    busy = true;
    let importedCount = 0;
    try {
      for (const att of supported) {
        const res = await invoke<PcoAttachmentImportResult>("pco_import_attachment", {
          appIdOrBearer: pcoAppId.trim(),
          secret: pcoSecret.trim() || null,
          attachment: att,
          targetLibraryId,
          targetMediaBinId,
          loadToDeck: false,
        });
        if (res.scene) onSceneChange?.(res.scene);
        importedCount += 1;
      }
      statusMsg = `Batch imported ${importedCount} Planning Center attachment(s) into chosen libraries & media bins.`;
    } catch (err) {
      errorMsg = `Imported ${importedCount} item(s) before error: ${String(err)}`;
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    void loadTargets();
  });
</script>

<div class="pco-pdf-panel" data-testid="pco-pdf-importer">
  <h3>Lyric Sheet PDF Parser & Planning Center (PCO) Auto-Importer</h3>

  {#if errorMsg}
    <div class="alert err" role="alert">{errorMsg}</div>
  {/if}
  {#if statusMsg}
    <div class="alert ok" role="status">{statusMsg}</div>
  {/if}

  <div class="row targets-row">
    <label class="field">
      <span>Target Lyrics Library</span>
      <select aria-label="Target Lyrics Library" bind:value={targetLibraryId}>
        {#if libraries.length === 0}
          <option value="worship-songs">Worship Songs</option>
          <option value="default">Default Library</option>
        {:else}
          {#each libraries as lib (lib.id)}
            <option value={lib.id}>{lib.name}</option>
          {/each}
        {/if}
      </select>
    </label>

    <label class="field">
      <span>Target Media Bin</span>
      <select aria-label="Target Media Bin" bind:value={targetMediaBinId}>
        {#if mediaBins.length === 0}
          <option value="backgrounds">Motion Backgrounds</option>
          <option value="images">Still Images & Slides</option>
          <option value="videos">Videos & Countdowns</option>
          <option value="audio">Audio & Walk-In Music</option>
        {:else}
          {#each mediaBins as bin (bin.id)}
            <option value={bin.id}>{bin.name}</option>
          {/each}
        {/if}
      </select>
    </label>

    <label class="check-field">
      <input type="checkbox" bind:checked={loadPdfToDeck} />
      <span>Load PDF to Deck</span>
    </label>
  </div>

  <div class="sub-card">
    <div class="sub-head">1. Local Presentation & Lyrics Import (PDF, Images / Folder, PowerPoint .pptx, Keynote .key) & Safe Media</div>
    <div class="row">
      <input
        aria-label="Local Presentation File Path"
        placeholder="Local file or image folder path (.pdf, .pptx, .key, .png/.jpg, folder, .show, .pro6)"
        bind:value={pdfFilePath}
      />
      <button class="btn primary" disabled={busy} onclick={() => void importLocalFile()}>
        Import Presentation / Lyrics
      </button>
    </div>
    <div class="row">
      <input
        aria-label="Media Context Label"
        placeholder="Presentation / Plan name (prevents filename collisions)"
        bind:value={mediaContextLabel}
        style="max-width:220px;"
      />
      <input
        aria-label="Safe Media File Path"
        placeholder="Local media path (.mp4, .png, .jpg, .mp3) → auto-named with BLAKE3 suffix"
        bind:value={mediaFilePath}
      />
      <button class="btn" disabled={busy} onclick={() => void importLocalMediaSafely()}>
        Import Media Safely
      </button>
    </div>
  </div>

  <div class="sub-card">
    <div class="sub-head">2. Planning Center Online (Services v2) — Download Lyric PDFs & Media Attachments</div>
    <div class="row">
      <input
        aria-label="PCO App ID or Bearer Token"
        placeholder="PCO App ID (or OAuth Bearer Token)"
        bind:value={pcoAppId}
      />
      <input
        aria-label="PCO Secret"
        type="password"
        placeholder="PCO Secret (leave blank for Bearer)"
        bind:value={pcoSecret}
      />
      <input
        aria-label="PCO Service Type ID"
        placeholder="Service Type ID"
        bind:value={pcoServiceTypeId}
        style="max-width:130px;"
      />
      <button class="btn primary" disabled={busy} onclick={() => void fetchPcoPlans()}>
        Fetch Plans
      </button>
    </div>

    {#if pcoPlans.length > 0}
      <ul class="item-list">
        {#each pcoPlans as plan (plan.id)}
          <li class:selected={selectedPlanId === plan.id}>
            <span>
              <strong>{plan.title}</strong>
              {#if plan.dates} · {plan.dates}{/if}
              ({plan.items.length} items)
            </span>
            <button class="btn xs" disabled={busy} onclick={() => void loadPlanAttachments(plan)}>
              Load Attachments
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    {#if pcoAttachments.length > 0}
      <div class="row att-bar">
        <span><strong>Attachments ({pcoAttachments.length})</strong> — media files receive meaningful, collision-proof names</span>
        <button class="btn xs primary" disabled={busy} onclick={() => void importAllSupportedAttachments()}>
          Import All PDFs & Media
        </button>
      </div>
      <ul class="item-list">
        {#each pcoAttachments as att (att.id)}
          <li>
            <span>
              <span class="kind-pill" class:pdf={att.kind === "LyricPdf"}>{att.kind.toUpperCase()}</span>
              <strong>{att.display_name}</strong>
              <small>({att.filename})</small>
            </span>
            <button
              class="btn xs"
              disabled={busy || att.kind === "Other"}
              onclick={() => void importAttachment(att)}
            >
              {att.kind === "LyricPdf" ? "Import Lyrics" : "Import Media"}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .pco-pdf-panel {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  h3 {
    margin: 0;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-1);
  }

  .sub-card {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .sub-head {
    font-size: 11px;
    font-weight: 700;
    color: var(--text-1);
  }

  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }

  .targets-row {
    align-items: flex-end;
  }

  .att-bar {
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-1);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 11px;
    color: var(--text-1);
    flex: 1;
    min-width: 150px;
  }

  .check-field {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-1);
    padding-bottom: 5px;
  }

  input,
  select {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 5px 8px;
    font-size: 12px;
    flex: 1;
  }

  .item-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .item-list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border-radius: 5px;
    background: var(--bg-2);
    font-size: 12px;
  }

  .item-list li.selected {
    border: 1px solid var(--accent);
  }

  .kind-pill {
    display: inline-block;
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 700;
    background: #1d4ed8;
    color: #fff;
    margin-right: 6px;
  }

  .kind-pill.pdf {
    background: #0d9488;
  }

  .btn {
    padding: 5px 10px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }

  .btn.xs {
    padding: 3px 8px;
    font-size: 11px;
  }

  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .alert {
    padding: 7px 10px;
    border-radius: 6px;
    font-size: 12px;
  }

  .alert.err {
    background: rgba(239, 68, 68, 0.18);
    color: #fecaca;
  }

  .alert.ok {
    background: rgba(34, 197, 94, 0.16);
    color: #bbf7d0;
  }
</style>
