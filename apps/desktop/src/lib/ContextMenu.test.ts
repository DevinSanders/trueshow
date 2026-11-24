import { fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import ContextMenu from "./ContextMenu.svelte";
import { closeContextMenu, openContextMenu } from "./contextMenuStore.svelte";

describe("ContextMenu & global browser menu suppression", () => {
  afterEach(() => {
    closeContextMenu();
  });

  it("suppresses generic browser right-click everywhere except text inputs, and runs custom context actions", async () => {
    render(ContextMenu);

    // 1. Generic right-click on non-input element is prevented (disables default WebView menu)
    const nonInputEvt = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    window.dispatchEvent(nonInputEvt);
    expect(nonInputEvt.defaultPrevented).toBe(true);
    expect(screen.queryByTestId("context-menu")).toBeNull();

    // 2. Right-click inside a text input is NOT prevented so copy/paste stays functional
    const input = document.createElement("input");
    document.body.appendChild(input);
    const inputEvt = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    input.dispatchEvent(inputEvt);
    expect(inputEvt.defaultPrevented).toBe(false);
    input.remove();

    // 3. Open custom context menu at bottom-right edge (verifies viewport clamping)
    const onSelect = vi.fn();
    const openEvt = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
      clientX: 4000,
      clientY: 4000,
    });
    openContextMenu(
      openEvt,
      [
        { id: "live", label: "Go Live", shortcut: "Enter" },
        { id: "off", label: "Disabled Action", disabled: true, separator: true },
      ],
      onSelect,
      "Slide 01",
    );
    expect(openEvt.defaultPrevented).toBe(true);

    const menu = await screen.findByTestId("context-menu");
    expect(menu.textContent).toContain("Slide 01");
    expect(menu.textContent).toContain("Enter");

    // Disabled item does not fire or close
    await fireEvent.click(screen.getByRole("menuitem", { name: "Disabled Action" }));
    expect(onSelect).not.toHaveBeenCalled();

    // Active item fires and closes menu
    await fireEvent.click(screen.getByRole("menuitem", { name: /Go Live/ }));
    expect(onSelect).toHaveBeenCalledWith("live");
    expect(screen.queryByTestId("context-menu")).toBeNull();

    // 4. Open and dismiss with Escape and empty-list guard
    openContextMenu(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      [{ id: "a", label: "Action A", danger: true }],
      onSelect,
    );
    expect(await screen.findByTestId("context-menu")).toBeTruthy();
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByTestId("context-menu")).toBeNull();

    openContextMenu(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true }),
      [],
      onSelect,
    );
    expect(screen.queryByTestId("context-menu")).toBeNull();
    // 5. Embedded cascading submenus ("Move To" / "Add to Playlist")
    const onSubSelect = vi.fn();
    openContextMenu(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 60, clientY: 60 }),
      [
        { id: "open", label: "Load into Cue Deck" },
        {
          id: "add-pl-menu",
          label: "Add to Playlist",
          children: [
            { id: "add-pl:sun", label: "Sunday Morning" },
            { id: "add-pl:wed", label: "Wednesday Night" },
          ],
        },
        {
          id: "move-menu",
          label: "Move To",
          children: [{ id: "move:worship", label: "Worship Songs" }],
        },
      ],
      onSubSelect,
      "Amazing Grace",
    );
    expect(await screen.findByTestId("context-menu")).toBeTruthy();

    // Hover opens submenu, hovering non-submenu sibling closes it
    const addPlWrap = screen.getByRole("menuitem", { name: "Add to Playlist" }).closest(".ctx-item-wrap")!;
    await fireEvent.mouseEnter(addPlWrap);
    expect(screen.getByTestId("submenu-add-pl-menu")).toBeTruthy();

    const openWrap = screen.getByRole("menuitem", { name: "Load into Cue Deck" }).closest(".ctx-item-wrap")!;
    await fireEvent.mouseEnter(openWrap);
    expect(screen.queryByTestId("submenu-add-pl-menu")).toBeNull();

    // Clicking submenu parent toggles it open, clicking nested item invokes onSelect & closes all
    await fireEvent.click(screen.getByRole("menuitem", { name: "Move To" }));
    expect(screen.getByTestId("submenu-move-menu")).toBeTruthy();
    await fireEvent.click(screen.getByRole("menuitem", { name: "Worship Songs" }));
    expect(onSubSelect).toHaveBeenCalledWith("move:worship");
    expect(screen.queryByTestId("context-menu")).toBeNull();
  });
});
