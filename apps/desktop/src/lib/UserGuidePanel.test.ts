import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import UserGuidePanel from "./UserGuidePanel.svelte";
import { parseGuideSections, renderMarkdownBlock, slugifyHeading } from "./markdown";

describe("markdown parser & offline UserGuidePanel", () => {
  it("renders headings, tables, lists, blockquotes, code blocks, links, images, and escapes HTML", () => {
    expect(slugifyHeading("20. Complete Keyboard Shortcuts Reference")).toBe(
      "20-complete-keyboard-shortcuts-reference",
    );

    const sample = [
      "# Guide Title",
      "Intro paragraph with **bold**, *italic*, `<code>`, [Jump](#sec-1), and [Docs](https://example.com).",
      "",
      "## Table of Contents",
      "- [Sec 1](#sec-1)",
      "",
      "## Sec 1",
      "### Subheading",
      "> A helpful tip <script>alert(1)</script>",
      "",
      "```text",
      "line <1>",
      "```",
      "",
      "| Key | Action |",
      "| :--- | :--- |",
      "| `F1` | Clear All |",
      "",
      "- Bullet one",
      "- Bullet two",
      "",
      "1. Step one",
      "2. Step two",
      "",
      "---",
      "",
      "![Demo Shot](images/demo.png)",
    ].join("\n");

    const parsed = parseGuideSections(sample, { "images/demo.png": "/bundled/demo.png" });
    expect(parsed.title).toBe("Guide Title");
    expect(parsed.introHtml).toContain("<strong>bold</strong>");
    expect(parsed.introHtml).toContain("<em>italic</em>");
    expect(parsed.introHtml).toContain("<code>&lt;code&gt;</code>");
    expect(parsed.introHtml).toContain('data-guide-anchor="sec-1"');
    expect(parsed.introHtml).toContain('href="https://example.com"');
    expect(parsed.sections).toHaveLength(1);

    const html = parsed.sections[0].html;
    expect(html).toContain('<h3 id="subheading">Subheading</h3>');
    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).toContain("<pre class=\"ug-pre\"><code>line &lt;1&gt;</code></pre>");
    expect(html).toContain("<table class=\"ug-table\">");
    expect(html).toContain("<ul><li>Bullet one</li><li>Bullet two</li></ul>");
    expect(html).toContain("<ol><li>Step one</li><li>Step two</li></ol>");
    expect(html).toContain("<hr />");
    expect(html).toContain('src="/bundled/demo.png"');

    // Also cover standalone renderMarkdownBlock with unmatched image
    expect(renderMarkdownBlock("![Alt](missing.png)")).toContain('src="missing.png"');
  });

  it("renders bundled USER_GUIDE.md offline with search, TOC jump, lightbox zoom, and close callback", async () => {
    const onClose = vi.fn();
    render(UserGuidePanel, {
      props: {
        initialAnchor: "20-complete-keyboard-shortcuts-reference",
        onClose,
      },
    });

    const panel = screen.getByTestId("user-guide-panel");
    expect(panel.textContent).toContain("TrueShow — Complete User Guide");
    expect(panel.textContent).toContain("Complete Keyboard Shortcuts Reference");

    // Verify real bundled screenshot images rendered inside the guide
    const imgs = panel.querySelectorAll("img");
    expect(imgs.length).toBeGreaterThanOrEqual(15);

    // Search filters sections cleanly and shows empty state when unmatched
    const searchInput = screen.getByLabelText("Search User Guide");
    await fireEvent.input(searchInput, { target: { value: "Planning Center" } });
    expect(panel.textContent).toContain("Planning Center");

    await fireEvent.input(searchInput, { target: { value: "zzzz-no-such-topic" } });
    expect(panel.textContent).toContain('No guide sections match "zzzz-no-such-topic"');

    // Clear search
    await fireEvent.click(screen.getByLabelText("Clear search"));
    expect(panel.querySelectorAll(".ug-toc-item").length).toBeGreaterThanOrEqual(15);

    // Click a TOC item and an internal guide anchor link
    const firstToc = panel.querySelector<HTMLButtonElement>(".ug-toc-item");
    expect(firstToc).toBeTruthy();
    await fireEvent.click(firstToc!);

    const anchorLink = panel.querySelector<HTMLAnchorElement>("[data-guide-anchor]");
    if (anchorLink) {
      await fireEvent.click(anchorLink);
    }

    // Open image lightbox by clicking a freshly queried screenshot, then dismiss via Escape and Close button
    const liveImg = panel.querySelector<HTMLImageElement>("img");
    expect(liveImg).toBeTruthy();
    await fireEvent.click(liveImg!);
    expect(screen.getByRole("dialog")).toBeTruthy();
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();

    await fireEvent.click(liveImg!);
    expect(screen.getByRole("dialog")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: /Close \(Esc\)/ }));
    expect(screen.queryByRole("dialog")).toBeNull();

    // Return to Show Deck
    await fireEvent.click(screen.getByRole("button", { name: "Back to Show Deck" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
