import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import CcliDisplaySettingsPanel from "./CcliDisplaySettingsPanel.svelte";

describe("CcliDisplaySettingsPanel (Automated First/Last Slide CCLI Attribution & Template)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("loads saved CCLI config, updates placement/license/template, inserts tokens, and saves live", async () => {
    const onSceneChange = vi.fn();
    let savedPayload: Record<string, unknown> = {};

    mockIPC((cmd, args) => {
      if (cmd === "get_ccli_display_config") {
        return {
          enabled: true,
          placement: "FirstSlide",
          church_license_number: "111222",
          template: "{title} • {author}\nCCLI Song #{ccli_song} • CCLI License #{ccli_license}",
          font_size: 24,
          text_color: { r: 1, g: 1, b: 1, a: 0.9 },
          background_color: { r: 0, g: 0, b: 0, a: 0.6 },
          bounds: { x: 80, y: 984, width: 1760, height: 76 },
          h_align: "Center",
        };
      }
      if (cmd === "save_ccli_display_config") {
        savedPayload = (args as Record<string, unknown>).config as Record<string, unknown>;
        return { revision: 2 };
      }
      return null;
    });

    render(CcliDisplaySettingsPanel, { props: { onSceneChange } });

    await waitFor(() =>
      expect((screen.getByLabelText("Church CCLI License Number") as HTMLInputElement).value).toBe("111222"),
    );

    await fireEvent.change(screen.getByLabelText("CCLI Slide Placement"), {
      target: { value: "LastSlide" },
    });
    await fireEvent.input(screen.getByLabelText("Church CCLI License Number"), {
      target: { value: "998877" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "{copyright}" }));
    expect(screen.getByTestId("ccli-preview").textContent).toContain("998877");

    await fireEvent.click(screen.getByRole("button", { name: "Save & Apply CCLI Template Live" }));
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("LastSlide"));
    expect(savedPayload.placement).toBe("LastSlide");
    expect(savedPayload.church_license_number).toBe("998877");
    expect(onSceneChange).toHaveBeenCalledTimes(1);
  });

  it("rejects empty templates and surfaces save errors cleanly", async () => {
    mockIPC((cmd) => {
      if (cmd === "get_ccli_display_config") return null;
      if (cmd === "save_ccli_display_config") throw new Error("engine offline");
      return null;
    });

    render(CcliDisplaySettingsPanel);

    await fireEvent.input(screen.getByLabelText("CCLI Display Template"), {
      target: { value: "   " },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save & Apply CCLI Template Live" }));
    expect(screen.getByRole("alert").textContent).toContain("cannot be empty");

    await fireEvent.input(screen.getByLabelText("CCLI Display Template"), {
      target: { value: "{title}" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save & Apply CCLI Template Live" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("engine offline"));
  });
});
