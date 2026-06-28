import { describe, it, expect } from "vitest";
import {
  projectTree,
  fileEntries,
  type DockEntry,
  type TreeNode,
} from "./project";

function file(path: string, dirty = false): DockEntry {
  return { key: path, name: path.split("/").pop()!, path, dirty };
}
function draft(key: string, name: string, dirty = true): DockEntry {
  return { key, name, path: null, dirty };
}

// Flatten the tree to indented rows for terse structural assertions.
function outline(nodes: TreeNode[], depth = 0): string[] {
  return nodes.flatMap((n) => {
    const row = `${"  ".repeat(depth)}${n.kind === "folder" ? "[" + n.name + "]" : n.name}`;
    return n.kind === "folder"
      ? [row, ...outline(n.children, depth + 1)]
      : [row];
  });
}

describe("fileEntries", () => {
  it("labels each file by its basename and carries path + dirty", () => {
    expect(
      fileEntries([
        { path: "rolls/forward.ctab", dirty: true },
        { path: "tune.ctab", dirty: false },
      ]),
    ).toEqual([
      {
        key: "rolls/forward.ctab",
        name: "forward.ctab",
        path: "rolls/forward.ctab",
        dirty: true,
      },
      { key: "tune.ctab", name: "tune.ctab", path: "tune.ctab", dirty: false },
    ]);
  });
});

describe("projectTree", () => {
  it("folds nested paths into folders, folders before files, each alphabetical", () => {
    const tree = projectTree([
      file("licks/roll.ctab"),
      file("licks/pinch.ctab"),
      file("drafts/sketch.ctab"),
      file("tune.ctab"),
    ]);
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
    const tree = projectTree([file("deep/one.ctab"), file("deep/two.ctab")]);
    const folders = tree.filter((n) => n.kind === "folder");
    expect(folders).toHaveLength(1);
    expect(folders[0].kind === "folder" && folders[0].children).toHaveLength(2);
  });

  it("nests multiple levels and keys folders by their full prefix", () => {
    const tree = projectTree([file("a/b/c.ctab")]);
    expect(outline(tree)).toEqual(["[a]", "  [b]", "    c.ctab"]);
    const a = tree[0];
    const b = a.kind === "folder" ? a.children[0] : null;
    expect(a.kind === "folder" && a.path).toBe("a");
    expect(b?.kind === "folder" && b.path).toBe("a/b");
  });

  it("renders path-null drafts as root leaves named by their entry name", () => {
    const tree = projectTree([
      file("licks/roll.ctab"),
      file("tune.ctab"),
      draft("draft:1", "untitled-1"),
    ]);
    expect(outline(tree)).toEqual([
      "[licks]",
      "  roll.ctab",
      "tune.ctab",
      "untitled-1",
    ]);
  });

  it("materializes empty folders from dirs (with their ancestors)", () => {
    const tree = projectTree(
      [file("licks/roll.ctab"), file("tune.ctab")],
      ["empty", "a/b", "licks"],
    );
    // `empty` and the nested `a/b` exist though they hold no files; `licks` from
    // dirs is the same node the file folds into (not duplicated).
    expect(outline(tree)).toEqual([
      "[a]",
      "  [b]",
      "[empty]",
      "[licks]",
      "  roll.ctab",
      "tune.ctab",
    ]);
    const licks = tree.filter((n) => n.kind === "folder" && n.name === "licks");
    expect(licks).toHaveLength(1);
  });

  it("normalizes and ignores blank dir keys", () => {
    const tree = projectTree([], ["licks\\rolls", "/", ""]);
    expect(outline(tree)).toEqual(["[licks]", "  [rolls]"]);
  });

  it("carries the entry through to the file leaf (key + dirty)", () => {
    const tree = projectTree([file("top.ctab", true)]);
    const top = tree.find((n) => n.kind === "file");
    expect(top?.kind === "file" && top.entry.key).toBe("top.ctab");
    expect(top?.kind === "file" && top.entry.dirty).toBe(true);
  });
});
