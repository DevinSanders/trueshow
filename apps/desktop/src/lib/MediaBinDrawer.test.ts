import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import MediaBinDrawer from "./MediaBinDrawer.svelte";
import type { MediaAssetRecord, MediaBinRecord } from "./types";

describe("MediaBinDrawer (Backgrounds, Images, Video, Audio)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("filters bins/categories, imports local media, triggers media live, and deletes items", async () => {
    let bins: MediaBinRecord[] = [
      { id: "backgrounds", name: "Motion Backgrounds", category: "background", updated_at: "2026-11-24T00:00:00Z" },
      { id: "audio", name: "Audio Tracks", category: "audio", updated_at: "2026-11-24T00:00:00Z" },
    ];
    let assets: MediaAssetRecord[] = [
      {
        blake3_hash: "h-aurora",
        bin_id: "backgrounds",
        file_name: "Aurora Wave (GPU Motion)",
        local_path: "builtin://motion/aurora-wave",
        kind: "background",
        width: 1920,
        height: 1080,
        duration_ms: null,
        has_alpha: false,
        size_bytes: 0,
      },
      {
        blake3_hash: "h-walkin",
        bin_id: "audio",
        file_name: "Sunday Walk-In.mp3",
        local_path: "C:\\Media\\walkin.mp3",
        kind: "audio",
        width: null,
        height: null,
        duration_ms: 180000,
        has_alpha: false,
        size_bytes: 4096,
      },
    ];

    const onMediaTriggered = vi.fn();
    const onError = vi.fn();

    mockIPC((cmd, args) => {
      const payload = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "list_media_bins":
          return bins;
        case "create_media_bin":
          bins = [
            ...bins,
            {
              id: "easter",
              name: String(payload.name),
              category: String(payload.category),
              updated_at: "2026-11-24T00:00:00Z",
            },
          ];
          return bins;
        case "delete_media_bin":
          bins = bins.filter((b) => b.id !== payload.binId);
          return bins;
        case "list_media_assets":
          return assets.filter((a) => {
            if (payload.kind && a.kind !== payload.kind) return false;
            if (payload.binId && a.bin_id !== payload.binId) return false;
            return true;
          });
        case "add_media_file": {
          const rec: MediaAssetRecord = {
            blake3_hash: "h-custom",
            bin_id: "backgrounds",
            file_name: "countdown.mp4",
            local_path: String(payload.filePath),
            kind: "video",
            width: 1920,
            height: 1080,
            duration_ms: null,
            has_alpha: Boolean(payload.hasAlpha),
            size_bytes: 1024,
          };
          assets = [...assets, rec];
          return rec;
        }
        case "trigger_media_asset":
          return { revision: 3, active_slide: null };
        case "delete_media_asset":
          assets = assets.filter((a) => a.blake3_hash !== payload.blake3Hash);
          return true;
        case "move_media_to_bin":
          assets = assets.map((a) =>
            a.blake3_hash === payload.blake3Hash ? { ...a, bin_id: String(payload.binId) } : a,
          );
          return true;
        default:
          return null;
      }
    });

    render(MediaBinDrawer, { props: { onMediaTriggered, onError } });

    await waitFor(() => expect(screen.getByText("Aurora Wave (GPU Motion)")).toBeTruthy());

    // 1. Trigger a motion background live (both auto, explicit BG, and explicit FG roles)
    await fireEvent.click(screen.getByText("Aurora Wave (GPU Motion)"));
    await fireEvent.click(screen.getByLabelText("Set Aurora Wave (GPU Motion) as Background Layer"));
    await fireEvent.click(screen.getByLabelText("Show Aurora Wave (GPU Motion) on Foreground Slide Layer"));
    await waitFor(() => expect(onMediaTriggered).toHaveBeenCalledTimes(3));

    // Drag media asset onto another Media Bin
    const mediaCard = screen.getByText("Aurora Wave (GPU Motion)").closest(".asset-card")!;
    await fireEvent.dragStart(mediaCard, { dataTransfer: { setData: vi.fn() } });
    const audioBinRow = screen.getByText("Audio Tracks").closest("li")!;
    await fireEvent.dragOver(audioBinRow);
    await fireEvent.dragLeave(audioBinRow);
    await fireEvent.drop(audioBinRow, {
      dataTransfer: {
        getData: (t: string) =>
          t === "application/x-trueshow-media"
            ? JSON.stringify({ blake3_hash: "h-aurora" })
            : "",
      },
    });
    await fireEvent.drop(audioBinRow, {
      dataTransfer: { getData: () => "{bad-json" },
    });

    // 2. Create & delete a custom media bin
    await fireEvent.click(screen.getByText("+ Bin"));
    await fireEvent.input(screen.getByLabelText("New Media Bin Name"), {
      target: { value: "Easter Graphics" },
    });
    await fireEvent.click(screen.getByText("Add"));
    await waitFor(() => expect(screen.getByText("Easter Graphics")).toBeTruthy());
    await fireEvent.click(screen.getByText("Easter Graphics"));
    await fireEvent.click(screen.getByLabelText("Delete bin Easter Graphics"));

    // 3. Filter by category tab (Audio -> All)
    await fireEvent.click(screen.getByText("Audio"));
    await waitFor(() => expect(screen.getByText("Sunday Walk-In.mp3")).toBeTruthy());
    await fireEvent.click(screen.getByText("All"));

    // 4. Import a local media file and then delete it
    await fireEvent.click(screen.getByText("+ Add Media"));
    await fireEvent.input(screen.getByLabelText("Media File Path"), {
      target: { value: "C:\\Media\\countdown.mp4" },
    });
    await fireEvent.click(screen.getByText("Import"));
    await waitFor(() => expect(screen.getByText("countdown.mp4")).toBeTruthy());

    await fireEvent.click(screen.getByLabelText("Remove media countdown.mp4"));
    expect(onError).not.toHaveBeenCalled();
  });

  it("surfaces validation and IPC failures cleanly", async () => {
    const onMediaTriggered = vi.fn();
    const onError = vi.fn();

    mockIPC((cmd) => {
      if (cmd === "list_media_bins") throw new Error("media table unavailable");
      if (cmd === "add_media_file") throw new Error("file not found");
      return [];
    });

    render(MediaBinDrawer, { props: { onMediaTriggered, onError } });
    await waitFor(() =>
      expect(onError).toHaveBeenCalledWith(expect.stringContaining("media table unavailable")),
    );

    await fireEvent.click(screen.getByText("+ Bin"));
    await fireEvent.click(screen.getByText("Add"));
    expect(onError).toHaveBeenCalledWith("Media bin name cannot be empty.");

    await fireEvent.click(screen.getByText("+ Add Media"));
    await fireEvent.click(screen.getByText("Import"));
    expect(onError).toHaveBeenCalledWith("Enter a local file path to import media.");

    await fireEvent.input(screen.getByLabelText("Media File Path"), {
      target: { value: "C:\\missing.mp4" },
    });
    await fireEvent.click(screen.getByText("Import"));
    await waitFor(() =>
      expect(onError).toHaveBeenCalledWith(expect.stringContaining("file not found")),
    );
  });
});
