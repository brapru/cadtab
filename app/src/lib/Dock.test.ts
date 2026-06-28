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
