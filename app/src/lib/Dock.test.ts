import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import Dock from "./Dock.svelte";

describe("Dock", () => {
  it("shows the project name and lists the entry plus libs", () => {
    const { container } = render(Dock, {
      entryName: "tune.ctab",
      libs: { "lib.ctab": "def x() {}" },
      projectName: "proj.ctabz",
    });
    expect(container.querySelector(".dock-header")?.textContent).toBe(
      "proj.ctabz",
    );
    const names = [...container.querySelectorAll(".file-name")].map(
      (n) => n.textContent,
    );
    expect(names).toEqual(["lib.ctab", "tune.ctab"]);
  });

  it("marks the file matching activePath as active", () => {
    const { container } = render(Dock, {
      entryName: "tune.ctab",
      libs: { "lib.ctab": "" },
      activePath: "lib.ctab",
    });
    const active = [
      ...container.querySelectorAll(".file.active .file-name"),
    ].map((n) => n.textContent);
    expect(active).toEqual(["lib.ctab"]);
  });

  it("fires onOpenFile with the path and entry flag on click", async () => {
    const onOpenFile = vi.fn();
    const { getByText } = render(Dock, {
      entryName: "tune.ctab",
      libs: { "lib.ctab": "" },
      onOpenFile,
    });
    await fireEvent.click(getByText("lib.ctab"));
    expect(onOpenFile).toHaveBeenCalledWith("lib.ctab", false);
    await fireEvent.click(getByText("tune.ctab"));
    expect(onOpenFile).toHaveBeenCalledWith("tune.ctab", true);
  });

  it("defaults the header to 'Project'", () => {
    const { container } = render(Dock, { entryName: "untitled" });
    expect(container.querySelector(".dock-header")?.textContent).toBe(
      "Project",
    );
  });

  it("renders nested paths as folders over their files", () => {
    const { container } = render(Dock, {
      entryName: "tune.ctab",
      libs: { "licks/roll.ctab": "", "licks/pinch.ctab": "" },
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

  it("collapses a folder's files when its row is clicked", async () => {
    const { container, getByText, queryByText } = render(Dock, {
      entryName: "tune.ctab",
      libs: { "licks/roll.ctab": "" },
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
});
