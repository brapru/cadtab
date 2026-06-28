import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import ContextMenu, { type ContextMenuItem } from "./ContextMenu.svelte";

const items: ContextMenuItem[] = [
  { label: "New File", action: "new-file" },
  { label: "New Folder", action: "new-folder" },
  {
    label: "Delete",
    action: "delete",
    destructive: true,
    separatorBefore: true,
  },
];

describe("ContextMenu", () => {
  it("renders the given items with a separator and destructive styling", () => {
    const { container, getByText } = render(ContextMenu, {
      x: 10,
      y: 20,
      items,
    });
    const labels = [...container.querySelectorAll(".item")].map((b) =>
      b.textContent?.trim(),
    );
    expect(labels).toEqual(["New File", "New Folder", "Delete"]);
    expect(container.querySelector(".sep")).toBeTruthy();
    expect(getByText("Delete").classList.contains("destructive")).toBe(true);
  });

  it("fires onSelect with the item's action when clicked", async () => {
    const onSelect = vi.fn();
    const { getByText } = render(ContextMenu, { x: 0, y: 0, items, onSelect });
    await fireEvent.click(getByText("New Folder"));
    expect(onSelect).toHaveBeenCalledWith("new-folder");
  });

  it("dismisses on a pointer down outside the menu", async () => {
    const onDismiss = vi.fn();
    render(ContextMenu, { x: 0, y: 0, items, onDismiss });
    await fireEvent.pointerDown(document.body);
    expect(onDismiss).toHaveBeenCalled();
  });

  it("does not dismiss on a pointer down inside the menu", async () => {
    const onDismiss = vi.fn();
    const { getByText } = render(ContextMenu, { x: 0, y: 0, items, onDismiss });
    await fireEvent.pointerDown(getByText("New File"));
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("dismisses on Escape", async () => {
    const onDismiss = vi.fn();
    render(ContextMenu, { x: 0, y: 0, items, onDismiss });
    await fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onDismiss).toHaveBeenCalled();
  });
});
