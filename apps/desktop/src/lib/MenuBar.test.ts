import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import MenuBar from "./MenuBar.svelte";
import StatusBar from "./StatusBar.svelte";
import type { EngineSnapshot } from "./types";

describe("MenuBar & StatusBar desktop chrome", () => {
  it("opens, switches on hover, closes on Escape/outside click, and invokes menu actions", async () => {
    const onSelectTab = vi.fn();
    const onToggleLibraryDrawer = vi.fn();
    const onToggleLayerDock = vi.fn();
    const onToggleMediaDrawer = vi.fn();
    const onUndo = vi.fn();
    const onRedo = vi.fn();
    const onToggleFreeze = vi.fn();
    const onReconnect = vi.fn();
    const onReloadUi = vi.fn();
    const onShutdownEngine = vi.fn();
    const onShutdownAll = vi.fn();

    render(MenuBar, {
      props: {
        activeTab: "broadcast",
        connected: true,
        frozen: false,
        crashCount: 2,
        showLibraryDrawer: true,
        showLayerDock: true,
        showMediaDrawer: false,
        onSelectTab,
        onToggleLibraryDrawer,
        onToggleLayerDock,
        onToggleMediaDrawer,
        onUndo,
        onRedo,
        onToggleFreeze,
        onReconnect,
        onReloadUi,
        onShutdownEngine,
        onShutdownAll,
      },
    });

    expect(screen.getByTestId("active-view-label").textContent).toBe("Broadcast I/O");

    // Open File menu, hover across to Tools, click Diagnostics (2)
    await fireEvent.click(screen.getByRole("menuitem", { name: "File" }));
    expect(screen.getByRole("menu", { name: "File menu" })).toBeTruthy();

    await fireEvent.mouseEnter(screen.getByRole("menuitem", { name: "Tools" }));
    expect(screen.getByRole("menu", { name: "Tools menu" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("menuitem", { name: /Diagnostics \(2\)/ }));
    expect(onSelectTab).toHaveBeenCalledWith("diagnostics");

    // Open Screens menu, close via Escape
    await fireEvent.click(screen.getByRole("menuitem", { name: "Screens" }));
    expect(screen.getByRole("menu", { name: "Screens menu" })).toBeTruthy();
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();

    // Open View menu, close via outside click
    await fireEvent.click(screen.getByRole("menuitem", { name: "View" }));
    expect(screen.getByRole("menu", { name: "View menu" })).toBeTruthy();
    await fireEvent.click(document.body);
    expect(screen.queryByRole("menu")).toBeNull();

    // Trigger Show quick-return button & Freeze toggle
    await fireEvent.click(screen.getByRole("button", { name: "Show" }));
    expect(onSelectTab).toHaveBeenCalledWith("show");
    await fireEvent.click(screen.getByRole("button", { name: "Freeze" }));
    expect(onToggleFreeze).toHaveBeenCalledTimes(1);

    // Trigger File -> Reconnect & Reload UI & Broadcast via Screens
    await fireEvent.click(screen.getByRole("menuitem", { name: "File" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: /Import & \.opbundle Backup/ }));
    expect(onSelectTab).toHaveBeenCalledWith("ecosystem");

    await fireEvent.click(screen.getByRole("menuitem", { name: "File" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: /Reconnect Engine/ }));
    expect(onReconnect).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByRole("menuitem", { name: "File" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: /Reload UI/ }));
    expect(onReloadUi).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByRole("menuitem", { name: "Screens" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: /Broadcast I\/O/ }));
    expect(onSelectTab).toHaveBeenCalledWith("broadcast");
  });

  it("renders compact bottom StatusBar for connected, frozen, blackout, and disconnected states", async () => {
    const onReconnect = vi.fn();
    const onOpenDiagnostics = vi.fn();
    const baseSnap = {
      protocol_version: 1,
      engine_pid: 9090,
      gpu_adapter_name: "NVIDIA RTX 5080",
      gpu_backend: "Vulkan",
      displays: [],
      preview_buses: [],
      telemetry: {
        fps: 59.9,
        avg_frame_ms: 16.5,
        p99_frame_ms: 16.7,
        total_frames: 100,
        dropped_frames: 0,
        outputs: [],
      },
      state: {
        revision: 1,
        blackout: false,
        freeze_output: true,
        outputs: [{ id: "1" }, { id: "2" }],
      },
    } as unknown as EngineSnapshot;

    const { rerender } = render(StatusBar, {
      props: {
        connected: false,
        statusMessage: "Engine disconnected",
        snapshot: null,
        telemetry: null,
        crashCount: 0,
        onReconnect,
        onOpenDiagnostics,
      },
    });

    expect(screen.getByTestId("status-bar").textContent).toContain("Engine disconnected");
    await fireEvent.click(screen.getByRole("button", { name: "Reconnect" }));
    expect(onReconnect).toHaveBeenCalledTimes(1);

    await rerender({
      connected: true,
      statusMessage: "Attached",
      snapshot: baseSnap,
      telemetry: baseSnap.telemetry,
      crashCount: 1,
      onReconnect,
      onOpenDiagnostics,
    });
    const bar = screen.getByTestId("status-bar");
    expect(bar.textContent).toContain("NVIDIA RTX 5080");
    expect(bar.textContent).toContain("59.9");
    expect(bar.textContent).toContain("FROZEN");
    expect(bar.textContent).toContain("2 outputs");
    await fireEvent.click(screen.getByRole("button", { name: "Diagnostics (1)" }));
    expect(onOpenDiagnostics).toHaveBeenCalledTimes(1);
  });
});
