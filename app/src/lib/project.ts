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
