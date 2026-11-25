import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import ShowControlPanel from "./ShowControlPanel.svelte";

describe("ShowControlPanel M3 UI (Happy & Failure Paths)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("creates timers, imports worship documents, and exports CCLI CSV (happy paths)", async () => {
    let mockTimers: { id: string; name: string; formatted: string; running: boolean; overrun: boolean }[] = [];
    mockIPC((cmd) => {
      if (cmd === "list_timers") return mockTimers;
      if (cmd === "list_macros") {
        return [{ id: "m-1", name: "Worship Start", steps: ["blackout:off"], updated_at: "2026-11-24T00:00:00Z" }];
      }
      if (cmd === "list_scheduled_events") {
        return [{ id: "ev-1", name: "Sunday 10am", trigger_hhmm: "10:00", macro_id: "m-1", enabled: true, updated_at: "2026-11-24T00:00:00Z" }];
      }
      if (cmd === "create_countdown_timer" || cmd === "control_timer" || cmd === "tick_timers") {
        mockTimers = [
          {
            id: "t-1",
            name: "Sermon Countdown",
            formatted: "30:00",
            running: cmd === "control_timer",
            overrun: false,
          },
        ];
        return mockTimers;
      }
      if (cmd === "save_macro" || cmd === "delete_macro") {
        return [{ id: "m-1", name: "Worship Start", steps: ["blackout:off"], updated_at: "2026-11-24T00:00:00Z" }];
      }
      if (cmd === "run_macro") return null;
      if (cmd === "save_scheduled_event" || cmd === "delete_scheduled_event") {
        return [{ id: "ev-1", name: "Sunday 10am", trigger_hhmm: "10:00", macro_id: "m-1", enabled: true, updated_at: "2026-11-24T00:00:00Z" }];
      }
      if (cmd === "dispatch_control_command") return "OSC executed";
      if (cmd === "import_worship_document") {
        return {
          title: "Amazing Grace",
          slide_count: 1,
          ccli_number: "22025",
          source_format: "ChordPro",
        };
      }
      if (cmd === "import_worship_file") {
        return {
          title: "Sunday Sermon",
          slide_count: 4,
          ccli_number: null,
          source_format: "PowerPoint OOXML (.pptx)",
        };
      }
      if (cmd === "export_library_backup") {
        return { bundle_path: "C:/backups/trueshow.tsbundle", item_count: 18 };
      }
      if (cmd === "import_library_backup") {
        return { bundle_path: "C:/backups/trueshow.tsbundle", item_count: 18 };
      }
      if (cmd === "export_ccli_csv_report") {
        return "ProjectedAt,ServiceProfile,SongTitle,CCLINumber,Author,DisplaySeconds\n2026-11-24,Sunday,Amazing Grace,22025,John Newton,180\n";
      }
      return null;
    });

    render(ShowControlPanel);

    // 1. Live Timers tab (default)
    expect(screen.getByTestId("section-timers")).toBeTruthy();
    await fireEvent.click(screen.getByText("Add Countdown"));
    await waitFor(() => expect(screen.getByRole("button", { name: "Start" })).toBeTruthy());
    await fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await fireEvent.click(screen.getByRole("button", { name: "+1m" }));
    await fireEvent.click(screen.getByRole("button", { name: "Reset" }));

    // 2. Macros & Schedule tab
    await fireEvent.click(screen.getByRole("tab", { name: "Macros & Schedule" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "▶ Worship Start" })).toBeTruthy());
    await fireEvent.click(screen.getByRole("button", { name: "Save Macro" }));
    await fireEvent.click(screen.getByRole("button", { name: "▶ Worship Start" }));
    await fireEvent.click(screen.getByLabelText("Delete macro Worship Start"));
    await fireEvent.click(screen.getByRole("button", { name: "Schedule" }));
    await fireEvent.click(screen.getByRole("button", { name: "Send" }));

    // 3. Import & PCO tab
    await fireEvent.click(screen.getByRole("tab", { name: "Import & PCO" }));
    await fireEvent.click(screen.getByText("Import to Local Library"));
    await waitFor(() =>
      expect(screen.getAllByRole("status").some((el) => el.textContent?.includes('Imported "Amazing Grace"'))).toBe(true),
    );

    await fireEvent.input(screen.getByLabelText("Import File Path"), {
      target: { value: "C:/sermons/sunday.pptx" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Import File" }));
    await waitFor(() =>
      expect(screen.getAllByRole("status").some((el) => el.textContent?.includes('Imported "Sunday Sermon"'))).toBe(true),
    );

    // 4. Backup tab
    await fireEvent.click(screen.getByRole("tab", { name: "Backup" }));
    await fireEvent.click(screen.getByRole("button", { name: "Export .tsbundle Backup" }));
    await waitFor(() =>
      expect(screen.getAllByRole("status").some((el) => el.textContent?.includes("Exported 18 library items"))).toBe(true),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Restore" }));
    await waitFor(() =>
      expect(screen.getAllByRole("status").some((el) => el.textContent?.includes("Restored 18 library items"))).toBe(true),
    );

    // 5. CCLI tab
    await fireEvent.click(screen.getByRole("tab", { name: "CCLI" }));
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

    // Import tab validation & backend failure
    await fireEvent.click(screen.getByRole("tab", { name: "Import & PCO" }));
    await fireEvent.input(screen.getByLabelText("Import Content"), { target: { value: "  " } });
    await fireEvent.click(screen.getByText("Import to Local Library"));
    expect(screen.getByRole("alert").textContent).toContain("Paste document content");

    await fireEvent.input(screen.getByLabelText("Import Content"), {
      target: { value: "{bad}" },
    });
    await fireEvent.click(screen.getByText("Import to Local Library"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("corrupt ChordPro syntax"),
    );

    // CCLI tab backend failure
    await fireEvent.click(screen.getByRole("tab", { name: "CCLI" }));
    await fireEvent.click(screen.getByText("Export CCLI CSV Report"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("database locked"),
    );
  });
});
