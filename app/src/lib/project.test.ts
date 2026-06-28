import { describe, it, expect } from "vitest";
import { projectFileList, projectTree, type TreeNode } from "./project";

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

// Flatten the tree to "<kind>:<name>" rows, indented by depth, for terse
// structural assertions.
function outline(nodes: TreeNode[], depth = 0): string[] {
  return nodes.flatMap((n) => {
    const row = `${"  ".repeat(depth)}${n.kind === "folder" ? "[" + n.name + "]" : n.name}`;
    return n.kind === "folder"
      ? [row, ...outline(n.children, depth + 1)]
      : [row];
  });
}

describe("projectTree", () => {
  it("folds nested paths into folders, folders before files, each alphabetical", () => {
    const tree = projectTree(
      projectFileList("tune.ctab", {
        "licks/roll.ctab": "",
        "licks/pinch.ctab": "",
        "drafts/sketch.ctab": "",
      }),
    );
    expect(outline(tree)).toEqual([
      "[drafts]",
      "  sketch.ctab",
      "[licks]",
      "  pinch.ctab",
      "  roll.ctab",
      "tune.ctab",
    ]);
  });

  it("shares one folder node across files in the same directory", () => {
    const tree = projectTree(
      projectFileList("a.ctab", {
        "deep/one.ctab": "",
        "deep/two.ctab": "",
      }),
    );
    const folders = tree.filter((n) => n.kind === "folder");
    expect(folders).toHaveLength(1);
    expect(folders[0].kind === "folder" && folders[0].children).toHaveLength(2);
  });

  it("nests multiple levels and keys folders by their full prefix", () => {
    const tree = projectTree(projectFileList("a/b/c.ctab", {}));
    expect(outline(tree)).toEqual(["[a]", "  [b]", "    c.ctab"]);
    const a = tree[0];
    const b = a.kind === "folder" ? a.children[0] : null;
    expect(a.kind === "folder" && a.path).toBe("a");
    expect(b?.kind === "folder" && b.path).toBe("a/b");
  });

  it("carries the entry flag through to the file leaf", () => {
    const tree = projectTree(projectFileList("top.ctab", { "x.ctab": "" }));
    const top = tree.find((n) => n.kind === "file" && n.name === "top.ctab");
    expect(top?.kind === "file" && top.file.isEntry).toBe(true);
  });
});
