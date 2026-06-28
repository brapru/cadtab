import { describe, it, expect } from "vitest";
import { projectFileList } from "./project";

describe("projectFileList", () => {
  it("lists the entry plus libs, sorted by path, with the entry flagged", () => {
    const files = projectFileList("tune.ctab", {
      "rolls/forward.ctab": "def forward() {}",
      "lib.ctab": "def x() {}",
    });
    expect(files).toEqual([
      { path: "lib.ctab", name: "lib.ctab", isEntry: false },
      { path: "rolls/forward.ctab", name: "forward.ctab", isEntry: false },
      { path: "tune.ctab", name: "tune.ctab", isEntry: true },
    ]);
  });

  it("is just the entry when there are no libs", () => {
    expect(projectFileList("untitled", {})).toEqual([
      { path: "untitled", name: "untitled", isEntry: true },
    ]);
  });

  it("uses the basename for the display label of a nested path", () => {
    const files = projectFileList("a.ctab", { "sub/dir/b.ctab": "" });
    expect(files.find((f) => f.path === "sub/dir/b.ctab")?.name).toBe("b.ctab");
  });
});
