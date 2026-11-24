import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import PcoPdfImporterPanel from "./PcoPdfImporterPanel.svelte";

describe("PcoPdfImporterPanel (Lyric Sheet PDF, Collision-Safe Media & PCO Services v2)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("selects target libraries/bins, imports local PDF & safe media, and imports PCO plan attachments", async () => {
    const onSceneChange = vi.fn();
    let lastPdfArgs: Record<string, unknown> = {};
    let lastMediaArgs: Record<string, unknown> = {};
    let pcoImportCount = 0;

    mockIPC((cmd, args) => {
      const p = (args ?? {}) as Record<string, unknown>;
      if (cmd === "list_libraries") {
        return [
          { id: "default", name: "Default Library", updated_at: "2026-11-24T00:00:00Z" },
          { id: "worship-songs", name: "Worship Songs", updated_at: "2026-11-24T00:00:00Z" },
        ];
      }
      if (cmd === "list_media_bins") {
        return [
          { id: "backgrounds", name: "Motion Backgrounds", category: "background", updated_at: "2026-11-24T00:00:00Z" },
          { id: "videos", name: "Videos & Countdowns", category: "video", updated_at: "2026-11-24T00:00:00Z" },
        ];
      }
      if (cmd === "import_local_presentation_file") {
        lastPdfArgs = p;
        return {
          song_id: "song-pdf-1",
          library_id: String(p.libraryId),
          title: "Goodness Of God",
          author: "Jenn Johnson",
          ccli_number: "7117726",
          slide_count: 4,
          arrangement_count: 1,
          scene: null,
        };
      }
      if (cmd === "import_media_file_safe") {
        lastMediaArgs = p;
        return {
          asset: {
            blake3_hash: "abcd1234",
            bin_id: String(p.targetBinId),
            file_name: "Easter Opener — background.mp4",
            local_path: "/vault/Easter-Opener__background__abcd1234.mp4",
            kind: "video",
          },
          safe_file_name: "Easter-Opener__background__abcd1234.mp4",
          deduplicated: false,
        };
      }
      if (cmd === "pco_list_plans") {
        return [
          {
            id: "plan-100",
            title: "Sunday Morning",
            dates: "Nov 29, 2026",
            items: [{ id: "i1", title: "Goodness Of God", item_type: "song", song_ccli: "7117726", arrangement_key: "A", description: null }],
          },
        ];
      }
      if (cmd === "pco_list_plan_attachments") {
        return [
          {
            id: "att-pdf",
            filename: "GoodnessOfGod.pdf",
            display_name: "Goodness Of God Lyrics PDF",
            content_type: "application/pdf",
            file_size: 42000,
            kind: "LyricPdf",
            context_label: "Sunday Morning",
            open_endpoint: "https://api.planningcenteronline.com/services/v2/attachments/att-pdf/open",
            download_url: null,
          },
          {
            id: "att-vid",
            filename: "background.mp4",
            display_name: "Service Loop Video",
            content_type: "video/mp4",
            file_size: 900000,
            kind: "Video",
            context_label: "Sunday Morning",
            open_endpoint: "https://api.planningcenteronline.com/services/v2/attachments/att-vid/open",
            download_url: null,
          },
        ];
      }
      if (cmd === "import_chord_chart_file") {
        return {
          kind: "ChordChart",
          title: "Goodness Of God",
          count: 4,
          summary: 'Attached Chord Chart "Goodness Of God" (4 sections) to active song & Musician Stage',
          scene: null,
        };
      }
      if (cmd === "import_sheet_music_file") {
        return {
          kind: "SheetMusic",
          title: "Goodness Of God Piano Score",
          count: 3,
          summary: 'Attached Sheet Music "Goodness Of God Piano Score" (3 full-clef page(s)) to active song & Musician Stage',
          scene: null,
        };
      }
      if (cmd === "pco_import_attachment") {
        pcoImportCount += 1;
        return {
          kind: "LyricPdf",
          summary: `Imported attachment #${pcoImportCount} into library '${String(p.targetLibraryId)}' / bin '${String(p.targetMediaBinId)}'`,
          scene: null,
        };
      }
      return null;
    });

    render(PcoPdfImporterPanel, { props: { onSceneChange } });

    await waitFor(() => expect(screen.getByLabelText("Target Lyrics Library")).toBeTruthy());
    await fireEvent.change(screen.getByLabelText("Target Lyrics Library"), {
      target: { value: "worship-songs" },
    });
    await fireEvent.change(screen.getByLabelText("Target Media Bin"), {
      target: { value: "videos" },
    });

    // 1. Local Lyric Sheet PDF / PPTX / Keynote / Images import into selected library
    await fireEvent.input(screen.getByLabelText("Local Presentation File Path"), {
      target: { value: "C:/Songs/GoodnessOfGod.pdf" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Import Presentation / Lyrics" }));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain('Imported "Goodness Of God"'),
    );
    expect(lastPdfArgs.libraryId).toBe("worship-songs");

    // 2. Collision-safe local media import into selected media bin
    await fireEvent.input(screen.getByLabelText("Media Context Label"), {
      target: { value: "Easter Opener" },
    });
    await fireEvent.input(screen.getByLabelText("Safe Media File Path"), {
      target: { value: "C:/Media/background.mp4" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Import Media Safely" }));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("Easter-Opener__background__abcd1234.mp4"),
    );
    expect(lastMediaArgs.targetBinId).toBe("videos");

    // 2B. Local Chord Chart & Full-Clef Sheet Music imports
    await fireEvent.input(screen.getByLabelText("Chord Chart File Path"), {
      target: { value: "C:/Charts/GoodnessOfGod_Chords.pdf" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Import Chord Chart" }));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain('Attached Chord Chart "Goodness Of God"'),
    );

    await fireEvent.input(screen.getByLabelText("Sheet Music File Path"), {
      target: { value: "C:/Scores/GoodnessOfGod_Piano.pdf" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Import Sheet Music" }));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("3 full-clef page(s)"),
    );

    // 3. Planning Center Online plans -> attachments -> single & batch import
    await fireEvent.input(screen.getByLabelText("PCO App ID or Bearer Token"), {
      target: { value: "pco-app-123" },
    });
    await fireEvent.input(screen.getByLabelText("PCO Secret"), {
      target: { value: "pco-secret-xyz" },
    });
    await fireEvent.input(screen.getByLabelText("PCO Service Type ID"), {
      target: { value: "42" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Fetch Plans" }));
    await waitFor(() => expect(screen.getByText("Sunday Morning")).toBeTruthy());

    await fireEvent.click(screen.getByRole("button", { name: "Load Attachments" }));
    await waitFor(() => expect(screen.getByText("Goodness Of God Lyrics PDF")).toBeTruthy());

    await fireEvent.click(screen.getByRole("button", { name: "Lyrics" }));
    await waitFor(() => expect(pcoImportCount).toBe(1));
    await fireEvent.click(screen.getByRole("button", { name: "Chord Chart" }));
    await waitFor(() => expect(pcoImportCount).toBe(2));
    await fireEvent.click(screen.getByRole("button", { name: "Sheet Music" }));
    await waitFor(() => expect(pcoImportCount).toBe(3));

    await fireEvent.click(screen.getByRole("button", { name: "Import All PDFs & Media" }));
    await waitFor(() => expect(pcoImportCount).toBe(5));
  });

  it("validates blank fields and surfaces backend errors cleanly", async () => {
    mockIPC((cmd) => {
      if (cmd === "list_libraries" || cmd === "list_media_bins") return [];
      if (cmd === "import_local_presentation_file") throw new Error("invalid PDF header");
      return null;
    });

    render(PcoPdfImporterPanel);

    await fireEvent.click(screen.getByRole("button", { name: "Import Presentation / Lyrics" }));
    expect(screen.getByRole("alert").textContent).toContain("file or folder path");

    await fireEvent.click(screen.getByRole("button", { name: "Import Media Safely" }));
    expect(screen.getByRole("alert").textContent).toContain("media file path");

    await fireEvent.click(screen.getByRole("button", { name: "Fetch Plans" }));
    expect(screen.getByRole("alert").textContent).toContain("Planning Center App ID");

    await fireEvent.input(screen.getByLabelText("Local Presentation File Path"), {
      target: { value: "C:/bad.pdf" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Import Presentation / Lyrics" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("invalid PDF header"));
  });
});
