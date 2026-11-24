import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import BroadcastPanel from "./BroadcastPanel.svelte";

describe("BroadcastPanel UI (Happy & Failure Paths)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("loads broadcast runtime statuses and applies valid profiles, rejecting empty source names", async () => {
    mockIPC((cmd, args) => {
      if (cmd === "get_broadcast_capabilities") {
        return {
          ndi_status: "Not Installed (dynamic runtime required)",
          omt_ready: true,
          decklink_status: "Driver Detected (DeckLink Duo 2)",
          virtual_output_backend: "Spout2Dx11On12",
          web_stage_url: "http://192.168.1.50:5505/",
          web_remote_url: "http://192.168.1.50:5505/remote",
          remote_pin: "2026",
          active_source_name: "OpenPresenter — Main Output",
          alpha_mode: "StraightBgra",
          decklink_mode: "ExternalDualSdiKeyFill",
          streaming_active: false,
        };
      }
      if (cmd === "apply_broadcast_profile") {
        const p = (args ?? {}) as Record<string, unknown>;
        return {
          ndi_status: "Not Installed (dynamic runtime required)",
          omt_ready: true,
          decklink_status: "Driver Detected (DeckLink Duo 2)",
          virtual_output_backend: "Spout2Dx11On12",
          web_stage_url: "http://192.168.1.50:5505/",
          web_remote_url: "http://192.168.1.50:5505/remote",
          remote_pin: "2026",
          active_source_name: String(p.sourceName ?? ""),
          alpha_mode: String(p.alphaMode ?? "StraightBgra"),
          decklink_mode: String(p.decklinkMode ?? "ExternalDualSdiKeyFill"),
          streaming_active: Boolean(p.streamingActive),
        };
      }
      return null;
    });

    render(BroadcastPanel);
    await waitFor(() => expect(screen.getByText(/Spout2Dx11On12/)).toBeTruthy());
    expect(screen.getByText(/http:\/\/192\.168\.1\.50:5505\/remote/)).toBeTruthy();

    // Failure path: blank source name rejected
    await fireEvent.input(screen.getByLabelText("Broadcast Source Name"), {
      target: { value: "   " },
    });
    await fireEvent.click(screen.getByText("Apply Broadcast Profile"));
    expect(screen.getByRole("alert").textContent).toContain("cannot be empty");

    // Happy path: valid source name + Key/Fill alpha mode saved & stream toggled
    await fireEvent.input(screen.getByLabelText("Broadcast Source Name"), {
      target: { value: "Sanctuary Lower Thirds" },
    });
    await fireEvent.change(screen.getByLabelText("Alpha Mode"), {
      target: { value: "KeyFillSplit" },
    });
    await fireEvent.change(screen.getByLabelText("DeckLink Key Mode"), {
      target: { value: "InternalKeyer" },
    });
    await fireEvent.click(screen.getByText("Apply Broadcast Profile"));
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Sanctuary Lower Thirds"));

    await fireEvent.click(screen.getByText(/Start Broadcast Output/));
    await waitFor(() => expect(screen.getByText(/Stop Broadcast Output/)).toBeTruthy());
  });

  it("surfaces runtime probe errors cleanly", async () => {
    mockIPC((cmd) => {
      if (cmd === "get_broadcast_capabilities") {
        throw new Error("broadcast probe failed");
      }
      return null;
    });

    render(BroadcastPanel);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("broadcast probe failed"),
    );
  });
});
