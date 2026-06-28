import { basename } from "./io";

// One row in the project dock. A saved project file carries the `path` the
// import model keys it by; an unsaved draft has `path: null` and renders as a
// root leaf. `key` is the stable identity the dock reports back on open (a path
// for files, a draft id for drafts); `dirty` drives the unsaved dot.
export interface DockEntry {
  key: string;
  name: string;
  path: string | null;
  dirty: boolean;
}

// A file leaf in the dock tree, carrying its DockEntry for open/active/dirty.
export interface TreeFileNode {
  kind: "file";
  name: string;
  entry: DockEntry;
}

// A folder node, keyed by its accumulated path prefix so the dock can persist
// per-folder expand/collapse state across rebuilds.
export interface TreeFolderNode {
  kind: "folder";
  name: string;
  path: string;
  children: TreeNode[];
}

export type TreeNode = TreeFileNode | TreeFolderNode;

// The dock element a right-click acted on, reported to the host so it can decide
// where a New File/Folder lands (in a folder, else project root) and what a
// Rename/Delete targets. A draft (path-null) row reports as `root`.
export type DockTarget =
  | { kind: "folder"; path: string }
  | { kind: "file"; key: string; path: string }
  | { kind: "root" };

// An in-progress inline name edit in the dock tree: a phantom new row inside
// `parentPath` (empty string = project root), or a rename swapping a row's label
// for an input. Folder renames key by path; file renames by the entry key.
export type PendingEdit =
  | { kind: "new-file"; parentPath: string; initial: string }
  | { kind: "new-folder"; parentPath: string; initial: string }
  | { kind: "rename"; targetKey: string; isFolder: boolean; initial: string };

// Fold dock entries into a folder hierarchy by splitting each path on `/` (or
// `\`): every segment but the last is a nested folder, the last is the file
// leaf. Path-null entries (unsaved drafts) become root leaves named by `name`.
// Folders sort before files, each alphabetical by name (case-insensitive).
export function projectTree(entries: DockEntry[]): TreeNode[] {
  const roots: TreeNode[] = [];
  const folders = new Map<string, TreeFolderNode>();

  for (const entry of entries) {
    const segments = (entry.path ?? "").split(/[\\/]/).filter((s) => s !== "");
    let siblings = roots;
    let prefix = "";
    for (let i = 0; i < segments.length - 1; i++) {
      const seg = segments[i];
      prefix = prefix ? `${prefix}/${seg}` : seg;
      let folder = folders.get(prefix);
      if (!folder) {
        folder = { kind: "folder", name: seg, path: prefix, children: [] };
        folders.set(prefix, folder);
        siblings.push(folder);
      }
      siblings = folder.children;
    }
    siblings.push({ kind: "file", name: entry.name, entry });
  }

  sortNodes(roots);
  return roots;
}

function sortNodes(nodes: TreeNode[]): void {
  nodes.sort((a, b) =>
    a.kind !== b.kind
      ? a.kind === "folder"
        ? -1
        : 1
      : a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  );
  for (const n of nodes) if (n.kind === "folder") sortNodes(n.children);
}

// Build dock entries for a set of project files (path -> dirty), each labelled
// by its basename. Drafts are appended separately by the caller.
export function fileEntries(
  files: { path: string; dirty: boolean }[],
): DockEntry[] {
  return files.map((f) => ({
    key: f.path,
    name: basename(f.path),
    path: f.path,
    dirty: f.dirty,
  }));
}
