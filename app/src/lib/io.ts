import { isTauri } from "./core";

// Open/save of single `.ctab` documents, backend-agnostic by the same seam as
// `core.ts`: native dialogs + filesystem under Tauri (desktop), the browser's
// file picker + download under a plain browser (web). The pure path/name
// helpers are shared by both backends and unit-tested directly.

const CTAB_EXT = ".ctab";
const FILTERS = [{ name: "cadtab score", extensions: ["ctab"] }];

/** A document loaded from disk/picker: its filesystem path (desktop only; null
 *  on web, which has no persistent path), display name, and text contents. */
export type OpenedDoc = { path: string | null; name: string; content: string };
/** Where a save landed: the path (desktop) and display name. */
export type SaveResult = { path: string | null; name: string };
/** Where a save should go: an existing path to overwrite silently (desktop), or
 *  none — prompt with a Save dialog seeded by `suggestedName`. */
export type SaveTarget = { path: string | null; suggestedName: string };

/** The final path segment of a `/`- or `\`-separated path. */
export function basename(path: string): string {
  const segments = path.split(/[\\/]/);
  return segments[segments.length - 1] || path;
}

/** Ensure a filename carries the `.ctab` extension (case-insensitive check). */
export function withCtabExtension(name: string): string {
  const trimmed = name.trim();
  if (trimmed === "") return "untitled" + CTAB_EXT;
  return trimmed.toLowerCase().endsWith(CTAB_EXT)
    ? trimmed
    : trimmed + CTAB_EXT;
}

/** A default save name derived from the document's `title "..."` declaration,
 *  slugified, falling back to `untitled.ctab`. Seeds the save dialog/download. */
export function defaultDocName(source: string): string {
  const match = source.match(/^\s*title\s+"([^"]*)"/m);
  const slug = (match?.[1] ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return (slug || "untitled") + CTAB_EXT;
}

/** Open a `.ctab` document, or resolve null if the user cancels. */
export function openDocument(): Promise<OpenedDoc | null> {
  return isTauri() ? openTauri() : openWeb();
}

/** Save `content` to `target`. With an existing path (desktop), overwrites it
 *  silently; otherwise prompts a Save dialog (desktop) or downloads (web).
 *  Resolves null if the user cancels the dialog. */
export function saveDocument(
  content: string,
  target: SaveTarget,
): Promise<SaveResult | null> {
  return isTauri()
    ? saveTauri(content, target)
    : saveWeb(content, target.suggestedName);
}

async function openTauri(): Promise<OpenedDoc | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const { readTextFile } = await import("@tauri-apps/plugin-fs");
  const path = await open({ multiple: false, filters: FILTERS });
  if (typeof path !== "string") return null;
  return { path, name: basename(path), content: await readTextFile(path) };
}

async function saveTauri(
  content: string,
  target: SaveTarget,
): Promise<SaveResult | null> {
  const { writeTextFile } = await import("@tauri-apps/plugin-fs");
  // Known path: overwrite in place, no dialog. Otherwise prompt for one.
  let path = target.path;
  if (!path) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const chosen = await save({
      defaultPath: target.suggestedName,
      filters: FILTERS,
    });
    if (typeof chosen !== "string") return null;
    path = chosen;
  }
  await writeTextFile(path, content);
  return { path, name: basename(path) };
}

function openWeb(): Promise<OpenedDoc | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = CTAB_EXT;
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

function saveWeb(content: string, suggestedName: string): Promise<SaveResult> {
  const name = withCtabExtension(suggestedName);
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  URL.revokeObjectURL(url);
  return Promise.resolve({ path: null, name });
}
