import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import ScriptureDrawer from "./ScriptureDrawer.svelte";
import type { BibleVerseRecord, SceneState } from "./types";

const samplePrimary: BibleVerseRecord[] = [
  {
    translation: "KJV",
    book: "John",
    chapter: 3,
    verse: 16,
    text: "For God so loved the world...",
    attribution: "Public Domain",
    cached_at_epoch: null,
  },
];

const sampleSecondary: BibleVerseRecord[] = [
  {
    translation: "WEB",
    book: "John",
    chapter: 3,
    verse: 16,
    text: "For God so loved the world, that he gave his one and only Son...",
    attribution: "Public Domain",
    cached_at_epoch: null,
  },
];

describe("ScriptureDrawer (Offline FTS5 + Dual-Translation Lookup)", () => {
  afterEach(() => {
    clearMocks();
  });

  it("loads translations, searches references with dual-translation parallel view, and presents live", async () => {
    const onDeckLoaded = vi.fn();
    mockIPC((cmd, args) => {
      const p = (args ?? {}) as Record<string, unknown>;
      if (cmd === "list_bible_translations") return ["KJV", "WEB"];
      if (cmd === "lookup_scripture") {
        return {
          primary: samplePrimary,
          secondary: p.secondaryTranslation ? sampleSecondary : [],
        };
      }
      if (cmd === "present_scripture") {
        return { revision: 2, deck: [{ id: "s1" }] } as unknown as SceneState;
      }
      return null;
    });

    render(ScriptureDrawer, { props: { onDeckLoaded } });

    await waitFor(() => {
      expect(screen.getByText(/For God so loved the world/i)).toBeTruthy();
    });

    await fireEvent.change(screen.getByLabelText("Secondary Parallel Translation"), {
      target: { value: "WEB" },
    });
    await waitFor(() => {
      expect(screen.getByText(/one and only Son/i)).toBeTruthy();
    });

    await fireEvent.keyDown(screen.getByLabelText("Scripture Reference or Keyword"), { key: "Enter" });
    await fireEvent.click(screen.getByRole("button", { name: "Search" }));
    await fireEvent.click(screen.getByRole("button", { name: "Psalm 23:1-3" }));
    await fireEvent.click(screen.getByRole("button", { name: "Present Live" }));
    await waitFor(() => {
      expect(onDeckLoaded).toHaveBeenCalled();
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save & Present" }));
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain("Saved & loaded");
    });
  });

  it("handles empty search results and IPC errors gracefully", async () => {
    const onDeckLoaded = vi.fn();
    const onError = vi.fn();
    let searched = false;
    mockIPC((cmd) => {
      if (cmd === "list_bible_translations") throw new Error("offline");
      if (cmd === "lookup_scripture") {
        searched = true;
        return { primary: [], secondary: [] };
      }
      throw new Error("boom");
    });

    render(ScriptureDrawer, { props: { onDeckLoaded, onError } });
    await waitFor(() => {
      expect(searched).toBe(true);
      expect((screen.getByRole("button", { name: "Present Live" }) as HTMLButtonElement).disabled).toBe(false);
    });
    await fireEvent.click(screen.getByRole("button", { name: "Present Live" }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain("No verses found");
    });
  });
});
