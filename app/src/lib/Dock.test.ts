import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import Dock from "./Dock.svelte";
import type { DockEntry } from "./project";

function file(path: string, dirty = false): DockEntry {
  return { key: path, name: path.split("/").pop()!, path, dirty };
}
function draft(key: string, name: string, dirty = true): DockEntry {
  return { key, name, path: null, dirty };
}

describe("Dock", () => {
  it("shows the project name and lists the files", () => {
    const { container } = render(Dock, {
      entries: [file("tune.ctab"), file("lib.ctab")],
      projectName: "proj.ctabz",
    });
    expect(container.querySelector(".dock-title")?.textContent).toBe(
      "proj.ctabz",
    );
    const names = [...container.querySelectorAll(".file .file-name")].map(
      (n) => n.textContent,
    );
    expect(names).toEqual(["lib.ctab", "tune.ctab"]);
  });

  it("marks the file matching activeKey as active", () => {
    const { container } = render(Dock, {
      entries: [file("tune.ctab"), file("lib.ctab")],
      activeKey: "lib.ctab",
    });
    const active = [
      ...container.querySelectorAll(".file.active .file-name"),
    ].map((n) => n.textContent);
    expect(active).toEqual(["lib.ctab"]);
  });

  it("fires onOpen with the clicked entry", async () => {
    const onOpen = vi.fn();
    const { getByText } = render(Dock, {
      entries: [file("tune.ctab"), file("lib.ctab")],
      onOpen,
    });
    await fireEvent.click(getByText("lib.ctab"));
    expect(onOpen).toHaveBeenCalledWith(
      expect.objectContaining({ key: "lib.ctab" }),
    );
  });

  it("defaults the header to 'Project'", () => {
    const { container } = render(Dock, { entries: [] });
    expect(container.querySelector(".dock-title")?.textContent).toBe("Project");
  });

  it("renders nested paths as folders over their files", () => {
    const { container } = render(Dock, {
      entries: [
        file("tune.ctab"),
        file("licks/roll.ctab"),
        file("licks/pinch.ctab"),
      ],
    });
    const folders = [...container.querySelectorAll(".folder .file-name")].map(
      (n) => n.textContent,
    );
    expect(folders).toEqual(["licks"]);
    const files = [...container.querySelectorAll(".file .file-name")].map(
      (n) => n.textContent,
    );
    expect(files).toEqual(["pinch.ctab", "roll.ctab", "tune.ctab"]);
  });

  it("anchors each folder's indent guide to the folder's depth", () => {
    const { container } = render(Dock, {
      entries: [file("licks/rolls/forward.ctab")],
    });
    // The guide is a ::before on the nested <ul>, positioned via --depth; assert
    // each nesting level carries its parent folder's depth (0 then 1). Visual
    // only otherwise (jsdom can't measure the pseudo-element).
    const depths = [...container.querySelectorAll("ul.nested")].map((ul) =>
      (ul as HTMLElement).style.getPropertyValue("--depth"),
    );
    expect(depths).toEqual(["0", "1"]);
  });

  it("collapses a folder's files when its row is clicked", async () => {
    const { container, getByText, queryByText } = render(Dock, {
      entries: [file("tune.ctab"), file("licks/roll.ctab")],
    });
    expect(getByText("roll.ctab")).toBeTruthy();
    const folder = container.querySelector(".folder") as HTMLElement;
    expect(folder.getAttribute("aria-expanded")).toBe("true");
    await fireEvent.click(folder);
    expect(folder.getAttribute("aria-expanded")).toBe("false");
    expect(queryByText("roll.ctab")).toBeNull();
    // the root entry stays visible — only the folder's contents hide
    expect(getByText("tune.ctab")).toBeTruthy();
  });

  it("shows an Open Folder control that fires onOpenFolder", async () => {
    const onOpenFolder = vi.fn();
    const { getByLabelText } = render(Dock, { entries: [], onOpenFolder });
    await fireEvent.click(getByLabelText("Open Folder"));
    expect(onOpenFolder).toHaveBeenCalled();
  });

  it("omits the Open Folder control when no callback is given", () => {
    const { queryByLabelText } = render(Dock, { entries: [] });
    expect(queryByLabelText("Open Folder")).toBeNull();
  });

  it("opens a context menu on right-click only when canManage is set", async () => {
    const closed = render(Dock, { entries: [file("tune.ctab")] });
    await fireEvent.contextMenu(closed.getByText("tune.ctab"));
    expect(closed.queryByText("New File")).toBeNull();
    closed.unmount();

    const open = render(Dock, {
      entries: [file("tune.ctab")],
      canManage: true,
    });
    await fireEvent.contextMenu(open.getByText("tune.ctab"));
    const items = [...open.container.querySelectorAll(".item")].map((i) =>
      i.textContent?.trim(),
    );
    expect(items).toEqual(["New File", "New Folder", "Rename", "Delete"]);
  });

  it("omits Rename/Delete when right-clicking empty space (root target)", async () => {
    const { container, getByLabelText } = render(Dock, {
      entries: [file("tune.ctab")],
      canManage: true,
    });
    await fireEvent.contextMenu(getByLabelText("Project files"));
    const items = [...container.querySelectorAll(".item")].map((i) =>
      i.textContent?.trim(),
    );
    expect(items).toEqual(["New File", "New Folder"]);
  });

  it("fires onContext with the action and the file target", async () => {
    const onContext = vi.fn();
    const { getByText } = render(Dock, {
      entries: [file("licks/roll.ctab")],
      canManage: true,
      onContext,
    });
    await fireEvent.contextMenu(getByText("roll.ctab"));
    await fireEvent.click(getByText("Rename"));
    expect(onContext).toHaveBeenCalledWith("rename", {
      kind: "file",
      key: "licks/roll.ctab",
      path: "licks/roll.ctab",
    });
  });

  it("renders an inline input for a pending new file and commits on Enter", async () => {
    const onCommitEdit = vi.fn();
    const { getByLabelText } = render(Dock, {
      entries: [file("tune.ctab")],
      canManage: true,
      pendingEdit: { kind: "new-file", parentPath: "", initial: "" },
      onCommitEdit,
    });
    const input = getByLabelText("Name") as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "  roll.ctab " } });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(onCommitEdit).toHaveBeenCalledWith("roll.ctab");
  });

  it("rejects empty or separator-bearing names on commit", async () => {
    const onCommitEdit = vi.fn();
    const { getByLabelText } = render(Dock, {
      entries: [file("tune.ctab")],
      canManage: true,
      pendingEdit: { kind: "new-folder", parentPath: "", initial: "" },
      onCommitEdit,
    });
    const input = getByLabelText("Name") as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "a/b" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    await fireEvent.input(input, { target: { value: "   " } });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(onCommitEdit).not.toHaveBeenCalled();
  });

  it("cancels the inline input on Escape", async () => {
    const onCancelEdit = vi.fn();
    const { getByLabelText } = render(Dock, {
      entries: [file("tune.ctab")],
      canManage: true,
      pendingEdit: { kind: "new-file", parentPath: "", initial: "" },
      onCancelEdit,
    });
    await fireEvent.keyDown(getByLabelText("Name"), { key: "Escape" });
    expect(onCancelEdit).toHaveBeenCalled();
  });

  it("swaps a renamed file's row for an input seeded with its name", () => {
    const { getByLabelText, queryByText } = render(Dock, {
      entries: [file("tune.ctab"), file("lib.ctab")],
      canManage: true,
      pendingEdit: {
        kind: "rename",
        targetKey: "lib.ctab",
        isFolder: false,
        initial: "lib.ctab",
      },
    });
    expect((getByLabelText("Name") as HTMLInputElement).value).toBe("lib.ctab");
    // the renamed row is replaced by the input; the other file stays a button
    expect(queryByText("lib.ctab")).toBeNull();
    expect(queryByText("tune.ctab")).toBeTruthy();
  });

  it("shows unsaved drafts as root leaves with a dirty dot", () => {
    const { container } = render(Dock, {
      entries: [file("tune.ctab"), draft("draft:1", "untitled-1")],
    });
    const dirty = [...container.querySelectorAll(".file.dirty .file-name")].map(
      (n) => n.textContent,
    );
    expect(dirty).toEqual(["untitled-1"]);
    expect(container.querySelectorAll(".dot")).toHaveLength(1);
  });
});
