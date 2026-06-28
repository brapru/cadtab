import { render } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
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

  it("marks only the entry document as active", () => {
    const { container } = render(Dock, {
      entryName: "tune.ctab",
      libs: { "lib.ctab": "" },
    });
    const active = [
      ...container.querySelectorAll(".file.active .file-name"),
    ].map((n) => n.textContent);
    expect(active).toEqual(["tune.ctab"]);
  });

  it("defaults the header to 'Project'", () => {
    const { container } = render(Dock, { entryName: "untitled" });
    expect(container.querySelector(".dock-header")?.textContent).toBe(
      "Project",
    );
  });
});
