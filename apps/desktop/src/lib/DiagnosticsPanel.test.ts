import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import DiagnosticsPanel from "./DiagnosticsPanel.svelte";
import type { CrashReport, DiagnosticsStatus, ExportedBundle } from "./types";

const sampleStatus: DiagnosticsStatus = {
  log_dir: "C:\\Users\\test\\AppData\\Local\\OpenPresenter\\logs",
  crash_dir: "C:\\Users\\test\\AppData\\Local\\OpenPresenter\\crashes",
  engine_log_path: "C:\\Users\\test\\AppData\\Local\\OpenPresenter\\logs\\engine.log",
  desktop_log_path: "C:\\Users\\test\\AppData\\Local\\OpenPresenter\\logs\\desktop.log",
  active_log_level: "info",
  crash_count: 1,
};

const sampleCrashes: CrashReport[] = [
  {
    id: "crash-1",
    timestamp_utc: "2026-11-24T16:00:00Z",
    component: "openpresenter-engine",
    kind: "wgpu-device-error",
    app_version: "0.1.0-m3",
    pid: 1234,
    os: "windows",
    arch: "x86_64",
    thread: "main",
    message: "Vulkan surface outdated during resize",
    location: "crates/render-wgpu/src/lib.rs:512",
    backtrace: "0: render_wgpu::render_output",
    recent_logs: ["INFO starting engine"],
  },
];

describe("DiagnosticsPanel (Happy & Failure Paths)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("loads logs and crash reports, switches log level, exports bundle, and clears reports", async () => {
    let currentLevel = "info";
    let currentCrashes = [...sampleCrashes];

    mockIPC((cmd, args) => {
      const payload = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "get_diagnostics_status":
          return { ...sampleStatus, active_log_level: currentLevel, crash_count: currentCrashes.length };
        case "list_crash_reports":
          return currentCrashes;
        case "read_diagnostic_logs":
          return payload.source === "engine"
            ? ["INFO engine frame ready"]
            : ["[engine] INFO engine frame ready", "[desktop] INFO UI attached"];
        case "set_debug_logging":
          currentLevel = String(payload.level);
          return currentLevel;
        case "export_diagnostics_bundle":
          return {
            file_path: "C:\\Logs\\openpresenter-diagnostics.md",
            crash_count: currentCrashes.length,
            markdown: "# OpenPresenter Diagnostic Bundle\n- Redacted OK",
          } satisfies ExportedBundle;
        case "clear_crash_reports": {
          const count = currentCrashes.length;
          currentCrashes = [];
          return count;
        }
        default:
          return null;
      }
    });

    render(DiagnosticsPanel);

    await waitFor(() => expect(screen.getByText(/Vulkan surface outdated/)).toBeTruthy());
    expect(screen.getByTestId("log-viewer").textContent).toContain("[desktop] INFO UI attached");

    // Switch log level to debug live
    await fireEvent.change(screen.getByLabelText("Log Verbosity Level"), {
      target: { value: "debug" },
    });
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain('Log level updated live to "debug"'),
    );

    // Filter log source to engine only
    await fireEvent.change(screen.getByLabelText("Log Source"), {
      target: { value: "engine" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("log-viewer").textContent).toBe("INFO engine frame ready"),
    );

    // Export redacted diagnostic bundle
    await fireEvent.click(screen.getByText("Export Diagnostic Bundle"));
    await waitFor(() => expect(screen.getByText("Redacted Bundle Preview")).toBeTruthy());

    // Clear crash reports
    await fireEvent.click(screen.getByText("Clear Reports"));
    await waitFor(() => expect(screen.getByText("No crash reports recorded.")).toBeTruthy());
  });

  it("surfaces explicit alerts on IPC failure paths for diagnostics actions", async () => {
    let failInitial = true;
    mockIPC((cmd) => {
      if (cmd === "get_diagnostics_status") {
        if (failInitial) throw new Error("log dir permission denied");
        return sampleStatus;
      }
      if (cmd === "list_crash_reports") return sampleCrashes;
      if (cmd === "read_diagnostic_logs") throw new Error("log file locked");
      if (cmd === "set_debug_logging") throw new Error("invalid log level");
      if (cmd === "export_diagnostics_bundle") throw new Error("disk full");
      if (cmd === "clear_crash_reports") throw new Error("cannot delete crash json");
      return null;
    });

    render(DiagnosticsPanel);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("log dir permission denied"),
    );

    failInitial = false;
    await fireEvent.change(screen.getByLabelText("Log Verbosity Level"), {
      target: { value: "trace" },
    });
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("invalid log level"),
    );

    await fireEvent.click(screen.getByText("Reload Tail"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("log file locked"),
    );

    await fireEvent.click(screen.getByText("Export Diagnostic Bundle"));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("disk full"));
  });
});
