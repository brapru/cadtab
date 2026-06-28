import { isTauri } from "./core";
import { parseBundle, serializeBundle, type ProjectBundle } from "./bundle";

// Open/save of cadtab documents and project bundles, backend-agnostic by the
// same seam as `core.ts`: native dialogs + filesystem under Tauri (desktop), the
// browser's file picker + download under a plain browser (web). The pure
// path/name helpers are shared by both backends and unit-tested directly.

const CTAB_EXT = ".ctab";
const BUNDLE_EXT = ".ctabz";
const SVG_EXT = ".svg";
const PNG_EXT = ".png";
const CADTAB_EXTS = [CTAB_EXT, BUNDLE_EXT];
const CTAB_FILTER = { name: "cadtab score", extensions: ["ctab"] };
const BUNDLE_FILTER = { name: "cadtab project", extensions: ["ctabz"] };
const SVG_FILTER = { name: "SVG image", extensions: ["svg"] };
const PNG_FILTER = { name: "PNG image", extensions: ["png"] };

type Filter = { name: string; extensions: string[] };

/// A document loaded from disk/picker: its filesystem path (desktop only; null
/// on web, which has no persistent path), display name, and text contents.
export type OpenedDoc = { path: string | null; name: string; content: string };
/// The unified Open result: a single score or a whole project bundle.
export type OpenedProject =
  | ({ kind: "single" } & OpenedDoc)
  | {
      kind: "bundle";
      path: string | null;
      name: string;
      bundle: ProjectBundle;
    };
/// Where a save landed: the path (desktop) and display name.
export type SaveResult = { path: string | null; name: string };
/// Where a save should go: an existing path to overwrite silently (desktop), or
/// none — prompt with a Save dialog seeded by `suggestedName`.
export type SaveTarget = { path: string | null; suggestedName: string };

/// The final path segment of a `/`- or `\`-separated path.
export function basename(path: string): string {
  const segments = path.split(/[\\/]/);
  return segments[segments.length - 1] || path;
}

/// The separator a path uses: a backslash only when it has one and no forward
/// slash (a Windows path), else a forward slash.
function pathSep(path: string): string {
  return path.includes("\\") && !path.includes("/") ? "\\" : "/";
}

/// Join a directory and a child name with the directory's own separator.
export function joinPath(dir: string, name: string): string {
  if (dir === "") return name;
  const tail = /[\\/]$/.test(dir) ? "" : pathSep(dir);
  return dir + tail + name;
}

/// A path relative to `root`, with the leading separator dropped and the result
/// normalized to forward slashes — the stable key the dock + import map use.
export function toRelative(root: string, abs: string): string {
  const rel = abs.startsWith(root) ? abs.slice(root.length) : abs;
  return rel.replace(/^[\\/]+/, "").replace(/\\/g, "/");
}

/// Ensure a filename carries `ext`, swapping a sibling cadtab extension if one
/// is present (so `tune.ctab` becomes `tune.ctabz`, not `tune.ctab.ctabz`).
function withExtension(name: string, ext: string): string {
  const trimmed = name.trim();
  if (trimmed === "") return "untitled" + ext;
  const lower = trimmed.toLowerCase();
  if (lower.endsWith(ext)) return trimmed;
  for (const other of CADTAB_EXTS) {
    if (other !== ext && lower.endsWith(other)) {
      return trimmed.slice(0, trimmed.length - other.length) + ext;
    }
  }
  return trimmed + ext;
}

/// Ensure a filename carries the `.ctab` extension.
export function withCtabExtension(name: string): string {
  return withExtension(name, CTAB_EXT);
}

/// A default save name derived from the document's `title "..."` declaration,
/// slugified, falling back to `untitled.ctab`. Seeds the save dialog/download.
export function defaultDocName(source: string): string {
  const match = source.match(/^\s*title\s+"([^"]*)"/m);
  const slug = (match?.[1] ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return (slug || "untitled") + CTAB_EXT;
}

/// Open a score (`.ctab`) or a project bundle (`.ctabz`), branching on the
/// picked file's extension. Resolves null if the user cancels; rejects if a
/// chosen bundle is malformed.
export async function openProject(): Promise<OpenedProject | null> {
  const picked = await pickFile([CTAB_FILTER, BUNDLE_FILTER]);
  if (!picked) return null;
  if (picked.name.toLowerCase().endsWith(BUNDLE_EXT)) {
    const { path, name, content } = picked;
    return { kind: "bundle", path, name, bundle: parseBundle(content) };
  }
  return { kind: "single", ...picked };
}

/// A directory entry as the fs backends report it (name + kind).
export interface DirEntry {
  name: string;
  isDirectory: boolean;
  isFile: boolean;
}

/// A live folder opened from disk: its root path, dock name, the file map (key
/// -> contents), and key -> absolute path for write-back. Keys are root-relative
/// forward-slash paths.
export type OpenedFolder = {
  root: string;
  name: string;
  files: Record<string, string>;
  filePaths: Record<string, string>;
};

/// Walk a directory tree from `root`, collecting every `.ctab` file: its
/// forward-slash key relative to root -> contents, plus key -> absolute path for
/// write-back. Dot-directories are skipped. The fs access is injected (the real
/// Tauri plugin in `openFolder`, fakes in tests), so the recursion stays pure.
export async function collectCtabFiles(
  root: string,
  readDir: (dir: string) => Promise<DirEntry[]>,
  readFile: (path: string) => Promise<string>,
): Promise<{
  files: Record<string, string>;
  filePaths: Record<string, string>;
}> {
  const files: Record<string, string> = {};
  const filePaths: Record<string, string> = {};
  async function walk(dir: string): Promise<void> {
    for (const entry of await readDir(dir)) {
      const child = joinPath(dir, entry.name);
      if (entry.isDirectory) {
        if (!entry.name.startsWith(".")) await walk(child);
      } else if (entry.isFile && entry.name.toLowerCase().endsWith(CTAB_EXT)) {
        const key = toRelative(root, child);
        files[key] = await readFile(child);
        filePaths[key] = child;
      }
    }
  }
  await walk(root);
  return { files, filePaths };
}

/// Open a whole project directory (desktop only — web has no folder access until
/// the FSA path lands). Picks a directory, then reads every `.ctab` under it.
/// Null when cancelled or off-desktop.
export async function openFolder(): Promise<OpenedFolder | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const root = await open({ directory: true });
  if (typeof root !== "string") return null;
  const { readDir, readTextFile } = await import("@tauri-apps/plugin-fs");
  const { files, filePaths } = await collectCtabFiles(
    root,
    (dir) => readDir(dir) as Promise<DirEntry[]>,
    readTextFile,
  );
  return { root, name: basename(root), files, filePaths };
}

/// Save a single score to `target`. Overwrites a known path silently (desktop),
/// else prompts a Save dialog (desktop) or downloads (web). Null if cancelled.
export function saveDocument(
  content: string,
  target: SaveTarget,
): Promise<SaveResult | null> {
  return writeFile(content, target, CTAB_EXT, [CTAB_FILTER]);
}

/// Save a whole project as a `.ctabz` bundle, same overwrite/prompt/download
/// rules as `saveDocument`.
export function saveBundle(
  bundle: ProjectBundle,
  target: SaveTarget,
): Promise<SaveResult | null> {
  return writeFile(serializeBundle(bundle), target, BUNDLE_EXT, [
    BUNDLE_FILTER,
  ]);
}

/// Export an SVG document (text) to `target`.
export function saveSvg(
  svg: string,
  target: SaveTarget,
): Promise<SaveResult | null> {
  return writeFile(svg, target, SVG_EXT, [SVG_FILTER]);
}

/// Export a PNG image (binary) to `target`: writes the bytes on desktop, or
/// downloads the blob in the browser.
export async function savePng(
  png: Blob,
  target: SaveTarget,
): Promise<SaveResult | null> {
  if (isTauri()) {
    const bytes = new Uint8Array(await png.arrayBuffer());
    return writeBinaryTauri(bytes, target, [PNG_FILTER]);
  }
  return downloadBlobWeb(png, target.suggestedName, PNG_EXT);
}

function pickFile(filters: Filter[]): Promise<OpenedDoc | null> {
  return isTauri() ? pickFileTauri(filters) : pickFileWeb(filters);
}

function writeFile(
  content: string,
  target: SaveTarget,
  ext: string,
  filters: Filter[],
): Promise<SaveResult | null> {
  return isTauri()
    ? writeFileTauri(content, target, filters)
    : downloadWeb(content, target.suggestedName, ext);
}

async function pickFileTauri(filters: Filter[]): Promise<OpenedDoc | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const { readTextFile } = await import("@tauri-apps/plugin-fs");
  const path = await open({ multiple: false, filters });
  if (typeof path !== "string") return null;
  return { path, name: basename(path), content: await readTextFile(path) };
}

async function writeFileTauri(
  content: string,
  target: SaveTarget,
  filters: Filter[],
): Promise<SaveResult | null> {
  const { writeTextFile } = await import("@tauri-apps/plugin-fs");
  // Known path: overwrite in place, no dialog. Otherwise prompt for one.
  let path = target.path;
  if (!path) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const chosen = await save({ defaultPath: target.suggestedName, filters });
    if (typeof chosen !== "string") return null;
    path = chosen;
  }
  await writeTextFile(path, content);
  return { path, name: basename(path) };
}

function pickFileWeb(filters: Filter[]): Promise<OpenedDoc | null> {
  const accept = filters
    .flatMap((f) => f.extensions.map((e) => "." + e))
    .join(",");
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.oncancel = () => resolve(null);
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) return resolve(null);
      void file
        .text()
        .then((content) => resolve({ path: null, name: file.name, content }));
    };
    input.click();
  });
}

function downloadWeb(
  content: string,
  suggestedName: string,
  ext: string,
): Promise<SaveResult> {
  return downloadBlobWeb(
    new Blob([content], { type: "text/plain;charset=utf-8" }),
    suggestedName,
    ext,
  );
}

async function writeBinaryTauri(
  bytes: Uint8Array,
  target: SaveTarget,
  filters: Filter[],
): Promise<SaveResult | null> {
  const { writeFile } = await import("@tauri-apps/plugin-fs");
  let path = target.path;
  if (!path) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const chosen = await save({ defaultPath: target.suggestedName, filters });
    if (typeof chosen !== "string") return null;
    path = chosen;
  }
  await writeFile(path, bytes);
  return { path, name: basename(path) };
}

function downloadBlobWeb(
  blob: Blob,
  suggestedName: string,
  ext: string,
): Promise<SaveResult> {
  const name = withExtension(suggestedName, ext);
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  URL.revokeObjectURL(url);
  return Promise.resolve({ path: null, name });
}
