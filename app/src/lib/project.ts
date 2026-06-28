import { basename } from "./io";

// One row in the project dock: a file in the open project, keyed by the path the
// import model uses (the entry name or a lib path), with a basename label and a
// flag for the entry document (the one currently in the editor).
export interface ProjectFile {
  path: string;
  name: string;
  isEntry: boolean;
}

// The project's files for the dock: the entry document plus its importable libs
// (the bundle map), sorted by path. The entry is flagged so the dock can
// mark the active file. Hierarchical folder rendering is a later refinement —
// bundles are flat today; this lists the structure that exists.
export function projectFileList(
  entry: string,
  libs: Record<string, string>,
): ProjectFile[] {
  const files: ProjectFile[] = [
    { path: entry, name: basename(entry), isEntry: true },
    ...Object.keys(libs).map((path) => ({
      path,
      name: basename(path),
      isEntry: false,
    })),
  ];
  return files.sort((a, b) => a.path.localeCompare(b.path));
}

// A file leaf in the dock tree, carrying its flat ProjectFile for open/active.
export interface TreeFileNode {
  kind: "file";
  name: string;
  file: ProjectFile;
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

// Fold a flat file list into a folder hierarchy by splitting each path on `/`
// (or `\`): every segment but the last is a nested folder, the last is the file
// leaf. Folders sort before files, each alphabetical by name (case-insensitive).
export function projectTree(files: ProjectFile[]): TreeNode[] {
  const roots: TreeNode[] = [];
  const folders = new Map<string, TreeFolderNode>();

  for (const file of files) {
    const segments = file.path.split(/[\\/]/).filter((s) => s !== "");
    if (segments.length === 0) continue;
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
    siblings.push({ kind: "file", name: file.name, file });
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
