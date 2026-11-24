import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import BroadcastPanel from "./BroadcastPanel.svelte";

describe("BroadcastPanel UI (Happy & Failure Paths)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("loads broadcast runtime statuses and applies valid profiles, rejecting empty source names", async () => {
    mockIPC((cmd) => {
      if (cmd === "get_broadcast_capabilities") {
        return {
          ndi_status: "Not Installed (dynamic runtime required)",
          omt_ready: true,
          decklink_status: "Driver Detected (DeckLink Duo 2)",
          virtual_output_backend: "Spout2Dx11On12",
          web_stage_url: "http://127.0.0.1:41900",
        };
      }
      return null;
    });

    render(BroadcastPanel);
    await waitFor(() => expect(screen.getByText(/Spout2Dx11On12/)).toBeTruthy());
    expect(screen.getByText(/http:\/\/127\.0\.0\.1:41900/)).toBeTruthy();

    // Failure path: blank source name rejected
    await fireEvent.input(screen.getByLabelText("Broadcast Source Name"), {
      target: { value: "   " },
    });
    await fireEvent.click(screen.getByText("Apply Broadcast Profile"));
    expect(screen.getByRole("alert").textContent).toContain("cannot be empty");

    // Happy path: valid source name + Key/Fill alpha mode saved
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
    expect(screen.getByRole("status").textContent).toContain("Sanctuary Lower Thirds");
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
