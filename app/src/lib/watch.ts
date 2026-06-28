// Reconciling a live folder's fresh scan against the open documents. Pure, so
// the watch wiring (Tauri `watch` → re-scan) stays thin glue and this is
// unit-tested directly.

export interface FolderScan {
  files: Record<string, string>; // key -> contents
  filePaths: Record<string, string>; // key -> absolute path
}

export interface FolderReconcile {
  files: Record<string, string>;
  filePaths: Record<string, string>;
  // Open project files whose disk content diverged from their buffer; the caller
  // swaps each tab to the disk content.
  reloads: { key: string; content: string }[];
}

// Always-reload semantics (disk is the source of truth): the scan becomes the
// project map outright — added files appear, deleted files drop — and any open
// file whose buffer differs from disk is queued for reload, even if it has
// unsaved edits. `openContent(key)` returns the open buffer for a file, or
// undefined when it isn't open. Drafts and files absent from disk are untouched.
export function reconcileScan(
  scan: FolderScan,
  openContent: (key: string) => string | undefined,
): FolderReconcile {
  const reloads: { key: string; content: string }[] = [];
  for (const [key, content] of Object.entries(scan.files)) {
    const buffer = openContent(key);
    if (buffer !== undefined && buffer !== content) {
      reloads.push({ key, content });
    }
  }
  return { files: scan.files, filePaths: scan.filePaths, reloads };
}
