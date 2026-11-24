import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import ShowControlPanel from "./ShowControlPanel.svelte";

describe("ShowControlPanel M3 UI (Happy & Failure Paths)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("creates timers, imports worship documents, and exports CCLI CSV (happy paths)", async () => {
    mockIPC((cmd) => {
      if (cmd === "create_countdown_timer") {
        return [
          {
            id: "t-1",
            name: "Sermon Countdown",
            formatted: "30:00",
            running: false,
            overrun: false,
          },
        ];
      }
      if (cmd === "import_worship_document") {
        return {
          title: "Amazing Grace",
          slide_count: 1,
          ccli_number: "22025",
          source_format: "ChordPro",
        };
      }
      if (cmd === "export_ccli_csv_report") {
        return "ProjectedAt,ServiceProfile,SongTitle,CCLINumber,Author,DisplaySeconds\n2026-11-24,Sunday,Amazing Grace,22025,John Newton,180\n";
      }
      return null;
    });

    render(ShowControlPanel);

    await fireEvent.click(screen.getByText("Add Countdown"));
    await waitFor(() => expect(screen.getByText("30:00")).toBeTruthy());

    await fireEvent.click(screen.getByText("Import to Local Library"));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain('Imported "Amazing Grace"'),
    );

    await fireEvent.click(screen.getByText("Export CCLI CSV Report"));
    await waitFor(() =>
      expect(screen.getByTestId("ccli-csv").textContent).toContain("CCLINumber"),
    );
  });

  it("validates blank fields and surfaces backend IPC failures cleanly (failure paths)", async () => {
    mockIPC((cmd) => {
      if (cmd === "create_countdown_timer") throw new Error("timer engine offline");
      if (cmd === "import_worship_document") throw new Error("corrupt ChordPro syntax");
      if (cmd === "export_ccli_csv_report") throw new Error("database locked");
      return null;
    });

    render(ShowControlPanel);

    // Client-side validation failure: empty timer name
    await fireEvent.input(screen.getByLabelText("Timer Name"), { target: { value: "   " } });
    await fireEvent.click(screen.getByText("Add Countdown"));
    expect(screen.getByRole("alert").textContent).toContain("required");

    // Backend failure: timer creation error
    await fireEvent.input(screen.getByLabelText("Timer Name"), { target: { value: "Prelude" } });
    await fireEvent.click(screen.getByText("Add Countdown"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("timer engine offline"),
    );

    // Client-side validation failure: empty import payload
    await fireEvent.input(screen.getByLabelText("Import Content"), { target: { value: "  " } });
    await fireEvent.click(screen.getByText("Import to Local Library"));
    expect(screen.getByRole("alert").textContent).toContain("Paste document content");

    // Backend failure: corrupt document
    await fireEvent.input(screen.getByLabelText("Import Content"), {
      target: { value: "{bad}" },
    });
    await fireEvent.click(screen.getByText("Import to Local Library"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("corrupt ChordPro syntax"),
    );

    // Backend failure: CCLI export failure
    await fireEvent.click(screen.getByText("Export CCLI CSV Report"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("database locked"),
    );
  });
});
