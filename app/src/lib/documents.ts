// Open-document sessions: the editor's model graduates from one global
// document to a keyed collection so `import`ed files can open as their own tabs.
// Pure data + operations; the reactive compile result lives in the UI layer,
// keyed by the same id.

// One open document: its identity, where Save writes, and the dirty baseline.
export interface DocSession {
  id: string;
  name: string | null; // display/file name; null = untitled
  path: string | null; // desktop fs path; null on web or never-saved
  content: string; // current editor buffer
  savedContent: string; // the text Save last wrote — what dirty compares against
  everSaved: boolean; // false = a never-saved draft, dirty until its first save
  missingOnDisk: boolean; // a project file deleted/moved out from under an open tab
}

// The open documents, in tab order, and which one has focus.
export interface DocStore {
  docs: DocSession[];
  activeId: string | null;
}

export function newSession(
  id: string,
  init: {
    name?: string | null;
    path?: string | null;
    content: string;
    everSaved?: boolean;
  },
): DocSession {
  return {
    id,
    name: init.name ?? null,
    path: init.path ?? null,
    content: init.content,
    savedContent: init.content,
    everSaved: init.everSaved ?? true,
    missingOnDisk: false,
  };
}

// A fresh store holding one document, focused.
export function singleDocStore(doc: DocSession): DocStore {
  return { docs: [doc], activeId: doc.id };
}

// Dirty iff the doc was never saved (an untitled draft, dirty until its first
// save) or its buffer has diverged from the last saved/opened text — so editing
// then undoing back to the baseline reads as clean again.
export function isDirty(doc: DocSession): boolean {
  return !doc.everSaved || doc.content !== doc.savedContent;
}

export function activeDoc(store: DocStore): DocSession | null {
  return store.docs.find((d) => d.id === store.activeId) ?? null;
}

// Insert or replace the session with `doc.id` (keeping tab order) and focus it.
// In the single-document phase this swaps the one open doc on open/new; once
// multiple docs open it adds a tab or re-focuses an already-open file.
export function putDoc(store: DocStore, doc: DocSession): DocStore {
  const exists = store.docs.some((d) => d.id === doc.id);
  const docs = exists
    ? store.docs.map((d) => (d.id === doc.id ? doc : d))
    : [...store.docs, doc];
  return { docs, activeId: doc.id };
}

// Remove a document session (its last view closed). Focus falls to the last
// remaining doc, or null when none are left. No-op when the id isn't open.
export function removeDoc(store: DocStore, id: string): DocStore {
  if (!store.docs.some((d) => d.id === id)) return store;
  const docs = store.docs.filter((d) => d.id !== id);
  const activeId =
    store.activeId === id
      ? (docs[docs.length - 1]?.id ?? null)
      : store.activeId;
  return { docs, activeId };
}

// Re-key a session (its backing file was renamed/moved): change its `id` and
// relabel its `name`/`path`, preserving the buffer, baseline, and flags so a
// rename never loses unsaved edits. The active pointer follows. A no-op if
// `oldId` isn't open.
export function renameDoc(
  store: DocStore,
  oldId: string,
  newId: string,
  patch: { name: string | null; path: string | null },
): DocStore {
  if (!store.docs.some((d) => d.id === oldId)) return store;
  const docs = store.docs.map((d) =>
    d.id === oldId
      ? { ...d, id: newId, name: patch.name, path: patch.path }
      : d,
  );
  const activeId = store.activeId === oldId ? newId : store.activeId;
  return { docs, activeId };
}

// Update the active document's editor buffer (dirty derives from the baseline).
export function setActiveContent(store: DocStore, content: string): DocStore {
  return mapDoc(store, store.activeId, (d) => ({ ...d, content }));
}

// Update a specific document's buffer — the multi-document form, since an edit
// belongs to whichever editor tab fired it, not necessarily the active one.
export function setDocContent(
  store: DocStore,
  id: string,
  content: string,
): DocStore {
  return mapDoc(store, id, (d) => ({ ...d, content }));
}

// Focus a document (active-follows-focus). Idempotent — returns the same store
// when the id is already active or isn't open — so focus events don't churn
// reactive state.
export function setActive(store: DocStore, id: string): DocStore {
  if (store.activeId === id || !store.docs.some((d) => d.id === id)) {
    return store;
  }
  return { ...store, activeId: id };
}

// Mark the active document saved: its current content becomes the new baseline,
// adopting the path/name it was written to.
export function markActiveSaved(
  store: DocStore,
  saved: { path: string | null; name: string | null },
): DocStore {
  return mapDoc(store, store.activeId, (d) => ({
    ...d,
    path: saved.path,
    name: saved.name,
    savedContent: d.content,
    everSaved: true,
    // The save (re)wrote the file, so it's back on disk.
    missingOnDisk: false,
  }));
}

// Reload a document's buffer from disk (a live-folder watch event): replace its
// content and rebaseline to it, so the reloaded text is clean and becomes the
// new undo baseline. No-op when the id isn't open.
export function reloadDoc(
  store: DocStore,
  id: string,
  content: string,
): DocStore {
  return mapDoc(store, id, (d) => ({
    ...d,
    content,
    savedContent: content,
    everSaved: true,
    missingOnDisk: false,
  }));
}

// Flag each open project file (`file:` doc) missing-on-disk per `isMissing(key)`
// — set when a folder scan no longer lists it (deleted/moved out from under an
// open tab), cleared when it reappears. Unchanged docs keep their reference so
// the reactive graph doesn't churn on every watch event.
export function markMissingOnDisk(
  store: DocStore,
  isMissing: (key: string) => boolean,
): DocStore {
  return {
    ...store,
    docs: store.docs.map((d) => {
      if (!d.id.startsWith("file:")) return d;
      const missing = isMissing(d.id.slice(5));
      return missing === d.missingOnDisk ? d : { ...d, missingOnDisk: missing };
    }),
  };
}

function mapDoc(
  store: DocStore,
  id: string | null,
  fn: (d: DocSession) => DocSession,
): DocStore {
  return { ...store, docs: store.docs.map((d) => (d.id === id ? fn(d) : d)) };
}
