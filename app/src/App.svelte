<script lang="ts">
  import Editor from "./lib/Editor.svelte";
  import RenderView from "./lib/RenderView.svelte";
  import PreviewView from "./lib/PreviewView.svelte";
  import Workspace from "./lib/Workspace.svelte";
  import BottomBar from "./lib/BottomBar.svelte";
  import Dock from "./lib/Dock.svelte";
  import ConfirmDialog from "./lib/ConfirmDialog.svelte";
  import Icon from "./lib/Icon.svelte";
  import {
    compile,
    paginate,
    completions as fetchCompletions,
    format as formatSource,
    isTauri,
  } from "./lib/core";
  import { emptyCompletions } from "./lib/completion";
  import { createLiveCompiler } from "./lib/live";
  import { debounce } from "./lib/debounce";
  import { byteToCharIndex, charToByteIndex, spanToRange } from "./lib/spans";
  import { narrowestSpanAt } from "./lib/mapping";
  import {
    defaultWorkspace,
    instance as viewInstance,
    addTab,
    closeTab,
    renameDoc as renameDocWorkspace,
    docIdsWithViews,
    groupOfType,
    activeTab,
    type Workspace as WorkspaceModel,
    type ViewInstance,
  } from "./lib/workspace";
  import {
    newSession,
    singleDocStore,
    activeDoc,
    isDirty,
    putDoc,
    removeDoc,
    renameDoc as renameDocSession,
    setDocContent,
    setActive,
    markActiveSaved,
    reloadDoc,
    markMissingOnDisk,
    type DocStore,
    type DocSession,
  } from "./lib/documents";
  import {
    fileEntries,
    type DockEntry,
    type DockTarget,
    type PendingEdit,
  } from "./lib/project";
  import { reconcileScan, type FolderScan } from "./lib/watch";
  import {
    layoutWidthForPx,
    clampZoom,
    ZOOM_STEP,
    PDF_CONTENT_WIDTH,
  } from "./lib/sizing";
  import { nextTheme, themeIcon, type Theme } from "./lib/theme";
  import {
    openProject,
    openFolder,
    rescanFolder,
    watchFolder,
    saveDocument,
    saveBundle,
    saveSvg,
    savePng,
    savePdf,
    defaultDocName,
    basename,
    resolvePath,
    withCtabExtension,
    createFile,
    createDir,
    removePath,
    renamePath,
  } from "./lib/io";
  import { renderTreeToSvg } from "./lib/svg";
  import { svgToPngBlob } from "./lib/png";
  import { tooltip } from "./lib/tooltip";
  import { TEMPLATES, templateById } from "./lib/templates";
  import type { CompileResult, Completions, Span } from "./lib/types";

  // A feature-rich starter so the app opens showing the header details row,
  // a time signature, beamed rhythms, and barlined measures end to end.
  const initialDoc = `title    "Cripple Creek"
composer "traditional"
tempo    120

instrument banjo
tuning     openG
capo "2"

score {
  time 4/4
  default 1/8

  3:0 2:0 1:0 5:0 3:0 2:0 1:0 5:0
  3:2 3:4 2:0 1:0 5:0 1:0 2:0 3:0
  3:0 2:0 1:0 5:0 3:0 2:0 1:0 5:0
}
`;

  // Per-document compile output, highlight, and layout width, keyed by doc id, so
  // several files' editors and renders coexist independently. Reassigned (not
  // mutated) so Svelte tracks the change.
  let results = $state<Record<string, CompileResult>>({});
  // Per-document completion vocabulary (T7.24), refreshed alongside compile so
  // the editor's autocomplete tracks the doc's own/imported defs as they change.
  let completionsByDoc = $state<Record<string, Completions>>({});
  let errors = $state<Record<string, string>>({});
  let selections = $state<Record<string, { from: number; to: number } | null>>(
    {},
  );
  let activeSpans = $state<Record<string, Span | null>>({});
  let layoutWidths = $state<Record<string, number>>({});

  // The open documents: each opened/imported file gets its own id, editor
  // tab, and render. The active doc drives the topbar name, Save/Export, and the
  // dirty indicator. Doc ids are scheme-prefixed: `file:<key>` for a project
  // file (the key is its dock/import path), `draft:<n>` for an unsaved draft.
  // The starter is a draft, but kept clean (everSaved) so the app doesn't open
  // looking unsaved.
  const initialId = "draft:0";
  let docStore = $state<DocStore>(
    singleDocStore(newSession(initialId, { content: initialDoc })),
  );
  let untitledCount = 0;
  const active = $derived(activeDoc(docStore));
  // The active doc's source, the spans the active result was compiled from, etc.
  const source = $derived(active?.content ?? "");
  const currentName = $derived(active?.name ?? null);
  const currentPath = $derived(active?.path ?? null);
  const dirty = $derived(active ? isDirty(active) : false);
  const activeResult = $derived(active ? (results[active.id] ?? null) : null);

  // True in the desktop (Tauri) webview: folder open + write-back are desktop
  // features; the topbar Open button is web-only (desktop opens via Cmd/Ctrl+O
  // and the dock's Open Folder, with the native menu to come in T7.30).
  const desktop = isTauri();

  // The open project: every file in it (key -> latest contents — the import map
  // shared by every compile), the fs path each key maps to (desktop write-back /
  // import base; empty for bundle/web), the live-folder root that gets watched
  // (desktop folders only; null for bundle/single/web), and the dock header name.
  let projectFiles = $state<Record<string, string>>({});
  let filePaths = $state<Record<string, string>>({});
  let projectDirs = $state<string[]>([]);
  let projectRoot = $state<string | null>(null);
  let projectName = $state("Project");

  // An in-progress inline name edit in the dock (New File/Folder or Rename),
  // driven by the dock's right-click menu. Set on a menu pick, cleared on
  // commit/cancel. The actual fs ops land in later sub-chunks (T7.36 2.3/2.4).
  let pendingEdit = $state<PendingEdit | null>(null);

  // Per-doc reload requests pushed into a live editor when a watched file
  // changes on disk: bumping the token swaps the CodeMirror state to the disk
  // content (resetting undo) without echoing back through onChange.
  let loadRequests = $state<Record<string, { content: string; token: number }>>(
    {},
  );
  let loadToken = 0;

  // Per-doc format requests pushed into a live editor (T7.25): bumping the token
  // replaces the buffer with the core-formatted text in one undoable transaction.
  let formatRequests = $state<
    Record<string, { content: string; token: number }>
  >({});
  let formatToken = 0;

  // The dock's rows: every project file (dirty iff its open doc has unsaved
  // edits) plus every open unsaved draft as a root leaf.
  const dockEntries = $derived.by(() => {
    const entries = fileEntries(
      Object.keys(projectFiles).map((key) => {
        const doc = docFor(`file:${key}`);
        return { path: key, dirty: doc ? isDirty(doc) : false };
      }),
    );
    for (const d of docStore.docs) {
      if (d.id.startsWith("draft:")) {
        entries.push({
          key: d.id,
          name: d.name ?? "untitled",
          path: null,
          dirty: isDirty(d),
        });
      }
    }
    return entries;
  });
  // The dock key of the active doc, so the dock marks the focused row: a file's
  // project key, or a draft's id.
  const activeKey = $derived(
    active?.id.startsWith("file:") ? active.id.slice(5) : (active?.id ?? null),
  );
  // Docs whose backing file was deleted/moved out from under their tab; the
  // workspace strikes their tab labels.
  const missingDocIds = $derived(
    docStore.docs.filter((d) => d.missingOnDisk).map((d) => d.id),
  );

  function docFor(id: string | null): DocSession | undefined {
    return id ? docStore.docs.find((d) => d.id === id) : undefined;
  }

  // A doc's display filename for its tab labels (D49); an unsaved draft with no
  // name yet reads as "untitled".
  function docName(id: string): string {
    return docFor(id)?.name ?? "untitled";
  }

  // One latest-wins compiler per document, so interleaved compiles never clobber
  // each other's render.
  const compilers: Record<string, ReturnType<typeof createLiveCompiler>> = {};
  function compilerFor(id: string) {
    let lc = compilers[id];
    if (!lc) {
      lc = createLiveCompiler(
        compile,
        (r) => {
          results = { ...results, [id]: r };
          errors = { ...errors, [id]: "" };
        },
        () => (errors = { ...errors, [id]: "core unavailable (no backend)" }),
      );
      compilers[id] = lc;
    }
    return lc;
  }

  // Compile one document at its own pane width and the shared project context,
  // and refresh its completion vocabulary from the same source + context.
  function compileDoc(id: string) {
    const doc = docFor(id);
    if (!doc) return;
    void compilerFor(id).run(
      doc.content,
      { width: layoutWidths[id] ?? 66 },
      { basePath: doc.path, files: projectFiles },
    );
    refreshCompletions(id);
  }

  // Latest-wins completion-vocabulary fetch per document: a stale resolution
  // (e.g. from an earlier keystroke) never clobbers a newer one, and a backend
  // error just leaves the previous vocabulary in place.
  const completionSeq: Record<string, number> = {};
  function refreshCompletions(id: string) {
    const doc = docFor(id);
    if (!doc) return;
    const mine = (completionSeq[id] = (completionSeq[id] ?? 0) + 1);
    void fetchCompletions(doc.content, {
      basePath: doc.path,
      files: projectFiles,
    })
      .then((c) => {
        if (mine === completionSeq[id]) {
          completionsByDoc = { ...completionsByDoc, [id]: c };
        }
      })
      .catch(() => {});
  }

  // Editing updates that document's buffer (dirty derives) and recompiles it. A
  // project-file edit also updates the shared import map and recompiles the
  // other open docs that may import it.
  function handleEdit(id: string, value: string) {
    docStore = setDocContent(docStore, id, value);
    if (id.startsWith("file:")) {
      const key = id.slice(5);
      // Only sync into the import map / dock while the file is actually in the
      // project; editing a doc whose file was deleted/moved (missing-on-disk)
      // mustn't resurrect a phantom dock row until it's saved back.
      if (key in projectFiles) {
        projectFiles = { ...projectFiles, [key]: value };
        for (const d of docStore.docs) if (d.id !== id) compileDoc(d.id);
      }
    }
    compileDoc(id);
  }

  // Debounced edit handler per document, so each editor keeps a stable callback.
  const editHandlers: Record<string, (value: string) => void> = {};
  function onChangeFor(id: string) {
    let h = editHandlers[id];
    if (!h) {
      h = debounce((value: string) => handleEdit(id, value), 150);
      editHandlers[id] = h;
    }
    return h;
  }

  // A render pane settled at a new width: re-lay-out that doc to fill it.
  function reflowDoc(id: string, px: number) {
    layoutWidths = { ...layoutWidths, [id]: layoutWidthForPx(px) };
    compileDoc(id);
  }

  // Render -> source: a clicked primitive selects its source range in that doc's
  // editor. Source -> render: the cursor lights the primitive(s) sharing its span.
  function handlePrimitiveClick(id: string, span: Span) {
    const doc = docFor(id);
    if (!doc) return;
    const range = spanToRange(byteToCharIndex(doc.content), span);
    if (range) selections = { ...selections, [id]: range };
  }
  function handleCursor(id: string, pos: number) {
    const r = results[id];
    const doc = docFor(id);
    if (!r || !doc) return;
    const byte = charToByteIndex(doc.content)[pos] ?? 0;
    activeSpans = { ...activeSpans, [id]: narrowestSpanAt(r.renderTree, byte) };
  }
  function clearHighlight(id: string) {
    activeSpans = { ...activeSpans, [id]: null };
  }

  // Active-follows-focus: focusing an editor or activating a tab makes its doc
  // the active one (topbar/Save/Export track it).
  function focusDoc(id: string) {
    docStore = setActive(docStore, id);
  }

  // The kind of view the user is focused on, so Cmd/Ctrl +/- zooms the right
  // thing — the editor's code font vs. the render's scale. The full instance is
  // also kept so Cmd/Ctrl-W can close whichever tab has focus.
  let focusedKind = $state<string>("editor");
  let focusedInstance = $state<ViewInstance | null>(null);
  function focusView(inst: ViewInstance) {
    if (inst.docId) focusDoc(inst.docId);
    focusedKind = inst.type;
    focusedInstance = inst;
  }

  // The workspace layout: the active doc's editor|render split. Opening a
  // file adds its editor and render as tabs next to the existing ones.
  let workspace = $state<WorkspaceModel>(defaultWorkspace(initialId));

  function addDocTabs(ws: WorkspaceModel, id: string): WorkspaceModel {
    const eg = groupOfType(ws, "editor") ?? ws.groups[0]?.id;
    const rg = groupOfType(ws, "render") ?? eg;
    if (eg) ws = addTab(ws, viewInstance("editor", id), eg);
    if (rg) ws = addTab(ws, viewInstance("render", id), rg);
    return ws;
  }

  // Drop every per-document derived map and cache. Used when opening a project
  // replaces the prior one, so a closed doc leaves nothing behind — no stale
  // render, no orphaned live compiler/edit handler. The new doc repopulates these
  // on its compile.
  function resetDocState() {
    results = {};
    completionsByDoc = {};
    errors = {};
    selections = {};
    activeSpans = {};
    layoutWidths = {};
    loadRequests = {};
    formatRequests = {};
    for (const k in compilers) delete compilers[k];
    for (const k in editHandlers) delete editHandlers[k];
  }

  // Drop a single closed document's derived maps and caches, so it leaves no
  // stale render/highlight or orphaned live compiler/edit handler behind. The
  // per-doc `$state` maps are reassigned (omitting the id) so Svelte tracks it.
  function without<T>(map: Record<string, T>, id: string): Record<string, T> {
    const { [id]: _drop, ...rest } = map;
    return rest;
  }
  function cleanupDoc(id: string) {
    results = without(results, id);
    completionsByDoc = without(completionsByDoc, id);
    formatRequests = without(formatRequests, id);
    errors = without(errors, id);
    selections = without(selections, id);
    activeSpans = without(activeSpans, id);
    layoutWidths = without(layoutWidths, id);
    delete compilers[id];
    delete editHandlers[id];
  }

  // A session for a project file, seeded from the import map and its fs path (if
  // any). The id carries the project key so the dock and edit-sync can find it.
  function fileSession(key: string): DocSession {
    return newSession(`file:${key}`, {
      name: basename(key),
      path: filePaths[key] ?? null,
      content: projectFiles[key] ?? "",
      everSaved: true,
    });
  }

  // Open a *project* (a single score, a bundle, or — Chunk B2 — a folder),
  // replacing the prior one: reset the import map/paths, drop every old doc, tab,
  // and render, and open one file as the first tab. So no stale render lingers
  // and the dock reflects only the new project.
  function openProjectInto(opts: {
    files: Record<string, string>;
    filePaths?: Record<string, string>;
    dirs?: string[];
    openKey: string | null;
    projectName: string;
    root?: string | null;
  }) {
    projectFiles = opts.files;
    filePaths = opts.filePaths ?? {};
    projectDirs = opts.dirs ?? [];
    projectRoot = opts.root ?? null;
    projectName = opts.projectName;
    resetDocState();
    // A folder opens with no file in the editor (`openKey` null): the dock shows
    // the tree and the workspace rests on its empty-tabs placeholder until the
    // user opens a file.
    if (opts.openKey === null) {
      docStore = { docs: [], activeId: null };
      workspace = { groups: [], maximizedId: null };
      return;
    }
    const doc = fileSession(opts.openKey);
    docStore = singleDocStore(doc);
    workspace = defaultWorkspace(doc.id);
    compileDoc(doc.id);
  }

  // Open (or focus) a project file as a tab within the current project — a dock
  // click. Adds tabs beside the open ones, or reseeds a layout when the
  // workspace was emptied; never replaces the project.
  function addOrFocusFile(key: string) {
    const id = `file:${key}`;
    if (!docStore.docs.some((d) => d.id === id)) {
      docStore = putDoc(docStore, fileSession(key));
      workspace =
        workspace.groups.length === 0
          ? defaultWorkspace(id)
          : addDocTabs(workspace, id);
    }
    focusDoc(id);
    compileDoc(id);
  }

  // Start a new unsaved draft tab in the current project (the tab-strip New
  // "+"). A draft is dirty from birth (never saved) and shows in the dock until
  // saved through the in-app flow.
  function newDraft(content: string) {
    const id = `draft:${++untitledCount}`;
    docStore = putDoc(docStore, newSession(id, { content, everSaved: false }));
    workspace =
      workspace.groups.length === 0
        ? defaultWorkspace(id)
        : addDocTabs(workspace, id);
    focusDoc(id);
    compileDoc(id);
  }

  // Open (or focus) a document-bound view as a tab beside the existing renders,
  // and focus its document. `addTab` is idempotent, so this spawns the view when
  // closed and jumps to it when already open. The view reuses the doc's live
  // compile result, so no extra compile.
  function openViewFor(docId: string, type: "render" | "preview") {
    const group =
      groupOfType(workspace, "render") ??
      groupOfType(workspace, "editor") ??
      workspace.groups[0]?.id;
    if (group) {
      workspace = addTab(workspace, viewInstance(type, docId), group);
    }
    focusDoc(docId);
  }

  // The active document's print preview, from the topbar Preview button.
  function openPreview() {
    if (active) openViewFor(active.id, "preview");
  }

  // Reopen a document's render from its editor tab's launcher — closes
  // the gap where a closed render had no way back.
  function openRender(docId: string) {
    openViewFor(docId, "render");
  }

  // Close a tab. Each view closes on its own — removing just that
  // instance, dropping a group it empties (like a move). A document's session
  // outlives its individual views and is cleaned up only once its *last* view
  // closes. Guard against losing unsaved work: closing the editor of a dirty doc
  // warns (its changes are no longer editable here), and closing a dirty doc's
  // last view warns that the changes are gone for good. After the close the
  // active document follows whatever view remains.
  async function closeView(inst: ViewInstance) {
    const docId = inst.docId;
    const after = closeTab(workspace, inst.id);
    const orphaned = docId !== null && !docIdsWithViews(after).has(docId);
    const doc = docId ? docFor(docId) : undefined;
    if (doc && isDirty(doc) && (inst.type === "editor" || orphaned)) {
      const name = doc.name ?? "untitled";
      const ok = await askConfirm({
        message: orphaned
          ? `Discard unsaved changes to ${name}? They will be lost.`
          : `Close the editor for ${name}? Its unsaved changes stay in the document's other open views.`,
        confirmLabel: orphaned ? "Discard & Close" : "Close editor",
        cancelLabel: "Keep open",
        destructive: orphaned,
      });
      if (!ok) return;
    }
    workspace = after;
    if (orphaned && docId) {
      cleanupDoc(docId);
      docStore = removeDoc(docStore, docId);
    }
    reconcileActive();
  }

  // Keep the active document pointing at a view that still exists after a close,
  // preferring the active tab of the first group that has one.
  function reconcileActive() {
    const live = docIdsWithViews(workspace);
    if (docStore.activeId && live.has(docStore.activeId)) return;
    for (const g of workspace.groups) {
      const t = activeTab(g);
      if (t?.docId && live.has(t.docId)) {
        docStore = setActive(docStore, t.docId);
        return;
      }
    }
  }

  // Project dock visibility, toggled from the bottom bar and Cmd/Ctrl-B.
  let dockOpen = $state(false);
  const toggleDock = () => (dockOpen = !dockOpen);

  // Cmd/Ctrl-B toggles the dock, overriding the browser's default for the key.
  function onDockKey(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return;
    if (e.key.toLowerCase() !== "b") return;
    toggleDock();
    e.preventDefault();
  }
  $effect(() => {
    window.addEventListener("keydown", onDockKey);
    return () => window.removeEventListener("keydown", onDockKey);
  });

  // Cmd/Ctrl-W closes the focused tab (and on desktop overrides the webview's
  // default of closing the whole window). Targets the focused view if it's still
  // open, else the active tab of the first group — so the shortcut always has a
  // sensible target while the workspace holds tabs.
  function closeFocusedTab() {
    const first = workspace.groups[0];
    const stillOpen =
      focusedInstance !== null &&
      workspace.groups.some((g) =>
        g.tabs.some((t) => t.id === focusedInstance!.id),
      );
    const focused = stillOpen
      ? focusedInstance
      : first
        ? activeTab(first)
        : null;
    if (focused) void closeView(focused);
  }
  function onCloseTabKey(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return;
    if (e.key.toLowerCase() !== "w") return;
    e.preventDefault();
    closeFocusedTab();
  }
  $effect(() => {
    window.addEventListener("keydown", onCloseTabKey);
    return () => window.removeEventListener("keydown", onCloseTabKey);
  });

  // Zoom is per view type: the editor's code font and the render's scale
  // each have their own level, and Cmd/Ctrl +/- targets whichever the user is
  // focused on. Render Fit (zoom 1) fills the pane width since layout reflows to
  // it; editor Fit returns the code font to its base size.
  let editorZoom = $state(1);
  let renderZoom = $state(1);
  const zoomTarget = () => (focusedKind === "editor" ? "editor" : "render");
  function zoomBy(factor: number) {
    if (zoomTarget() === "editor") editorZoom = clampZoom(editorZoom * factor);
    else renderZoom = clampZoom(renderZoom * factor);
  }
  const zoomIn = () => zoomBy(ZOOM_STEP);
  const zoomOut = () => zoomBy(1 / ZOOM_STEP);
  function zoomFit() {
    if (zoomTarget() === "editor") editorZoom = 1;
    else renderZoom = 1;
  }
  // The tab-strip Fit control is render-only, so it always resets the render.
  const fitRender = () => (renderZoom = 1);

  // Cmd/Ctrl +/- zoom the focused view and Cmd/Ctrl 0 fits it, overriding the
  // browser's native page zoom. `=`/`+` share a key (shift), as do `-`/`_`.
  function onZoomKey(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
    if (e.key === "=" || e.key === "+") zoomIn();
    else if (e.key === "-" || e.key === "_") zoomOut();
    else if (e.key === "0") zoomFit();
    else return;
    e.preventDefault();
  }
  $effect(() => {
    window.addEventListener("keydown", onZoomKey);
    return () => window.removeEventListener("keydown", onZoomKey);
  });

  // Theme: dark by default; "system" follows the OS; light/dark force a mode via
  // a root attribute the semantic CSS tokens key off. index.html pre-sets
  // data-theme="dark" so first paint matches this default with no light flash.
  let theme = $state<Theme>("dark");
  const cycleTheme = () => (theme = nextTheme(theme));
  $effect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
  });

  // Editor autocomplete + inline hints (T7.24c): on by default, toggled from the
  // bottom bar. Passed into every editor, which silences its completion popup when off.
  let autocomplete = $state(true);
  const toggleAutocomplete = () => (autocomplete = !autocomplete);

  // DSL formatter (T7.25): a bottom-bar toggle. When on, every save canonicalizes
  // the document through the core's pretty-printer first.
  let formatOnSave = $state(false);
  const toggleFormatOnSave = () => (formatOnSave = !formatOnSave);

  // Format one document through the core formatter and apply the result. Returns
  // the formatted text (or the original when unchanged / on failure), so the save
  // flow can write it without waiting on the editor's debounced echo.
  async function formatDoc(id: string): Promise<string | undefined> {
    const doc = docFor(id);
    if (!doc) return undefined;
    let formatted: string;
    try {
      formatted = await formatSource(doc.content);
    } catch {
      return doc.content; // formatter/backend hiccup: leave the text as-is
    }
    if (formatted === doc.content) return formatted;
    // Update the store now (immediate dirty/recompile) and push the change into
    // the live editor as an undoable replacement.
    handleEdit(id, formatted);
    formatRequests = {
      ...formatRequests,
      [id]: { content: formatted, token: ++formatToken },
    };
    return formatted;
  }

  // Start a new draft from a starter template as its own tab in the current
  // project — driven by the tab-strip New ("+") menu. No discard guard: New
  // never replaces the edited doc, it adds a tab.
  function newFromTemplate(id: string) {
    const template = templateById(id);
    if (!template) return;
    newDraft(template.source);
  }

  // Open a score (`.ctab`) or a whole project bundle (`.ctabz`) as a new tab (or
  // focus it if already open). A bundle sets the project context (entry + libs);
  // a single score opens with its own standalone context.
  // In-app confirmation modal: a single prompt at a time, holding the resolver its
  // buttons settle. Our own DOM dialog — cohesive with the UI, and it works in the
  // desktop WKWebView (which silently ignores the native `window.confirm`).
  let confirmPrompt = $state<{
    message: string;
    confirmLabel: string;
    cancelLabel: string;
    destructive: boolean;
    resolve: (ok: boolean) => void;
  } | null>(null);

  function askConfirm(opts: {
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    destructive?: boolean;
  }): Promise<boolean> {
    return new Promise((resolve) => {
      confirmPrompt = {
        message: opts.message,
        confirmLabel: opts.confirmLabel ?? "Confirm",
        cancelLabel: opts.cancelLabel ?? "Cancel",
        destructive: opts.destructive ?? false,
        resolve,
      };
    });
  }

  function settleConfirm(ok: boolean) {
    confirmPrompt?.resolve(ok);
    confirmPrompt = null;
  }

  // Opening a project replaces the current one, which can discard unsaved
  // work. Guard it: when the current project has dirty docs, confirm before going
  // further; otherwise swap silently. Checked before the file picker so a declined
  // prompt never opens a dialog.
  function confirmDiscardIfDirty(): Promise<boolean> {
    if (!docStore.docs.some(isDirty)) return Promise.resolve(true);
    return askConfirm({
      message:
        "Discard unsaved changes in the current project and open another?",
      confirmLabel: "Discard & Open",
      destructive: true,
    });
  }

  async function openFile() {
    if (!(await confirmDiscardIfDirty())) return;
    let opened;
    try {
      opened = await openProject();
    } catch (e) {
      window.alert(`Could not open project: ${(e as Error).message}`);
      return;
    }
    if (!opened) return;

    if (opened.kind === "single") {
      // A lone score is a one-file project, keyed by its fs path (desktop) or
      // name (web), so it lists in the dock like any project.
      const key = opened.path ?? opened.name;
      openProjectInto({
        files: { [key]: opened.content },
        filePaths: opened.path ? { [key]: opened.path } : {},
        openKey: key,
        projectName: opened.name,
      });
      return;
    }
    const { entry, files } = opened.bundle;
    openProjectInto({ files, openKey: entry, projectName: opened.name });
  }

  // Open a whole project directory as a live folder (desktop). Same discard
  // guard as opening a file; the dock then shows the real tree and saves write
  // back to the real files (each opened doc carries its fs path). A no-op
  // off-desktop, where `openFolder` resolves null (web folder access is later).
  async function openFolderFlow() {
    if (!(await confirmDiscardIfDirty())) return;
    let folder;
    try {
      folder = await openFolder();
    } catch (e) {
      // `window.alert` no-ops in WKWebView, so log too — a denied fs capability
      // would otherwise fail the open invisibly on desktop.
      console.error("open folder failed:", e);
      window.alert(`Could not open folder: ${(e as Error).message}`);
      return;
    }
    if (!folder) return;
    openProjectInto({
      files: folder.files,
      filePaths: folder.filePaths,
      dirs: folder.dirs,
      openKey: null,
      projectName: folder.name,
      root: folder.root,
    });
  }

  // Live folder (desktop): watch the open project's root and reconcile every
  // change. Re-runs when the root changes — opening another project tears down
  // the previous watch — and stops on unmount.
  $effect(() => {
    const root = projectRoot;
    if (!desktop || !root) return;
    let unwatch: (() => void) | undefined;
    let stopped = false;
    // `watchImmediate` fires per raw event (a single save can emit several), so
    // coalesce the re-scans here.
    const onChange = debounce(() => void onFolderChanged(root), 150);
    watchFolder(root, onChange)
      .then((u) => {
        if (stopped) u();
        else unwatch = u;
      })
      // Surface a denied/failed watch instead of silently not watching (e.g. a
      // missing fs capability) — the dock just won't live-update.
      .catch((e) => console.error("folder watch failed:", e));
    return () => {
      stopped = true;
      unwatch?.();
    };
  });

  // A watched file changed: re-scan the folder and reconcile (always-reload —
  // disk wins). Guarded against a project swap mid-scan.
  async function onFolderChanged(root: string) {
    let scan: FolderScan;
    try {
      scan = await rescanFolder(root);
    } catch (e) {
      console.error("folder re-scan failed:", e);
      return;
    }
    if (projectRoot !== root) return;
    applyScan(scan);
  }

  // Adopt a fresh folder scan: the project map becomes the scan (added files
  // appear, deleted drop), open files whose disk content diverged reload into
  // their tab, and every open project file recompiles (imports may have moved).
  function applyScan(scan: FolderScan) {
    const recon = reconcileScan(scan, (key) => docFor(`file:${key}`)?.content);
    projectFiles = recon.files;
    filePaths = recon.filePaths;
    projectDirs = recon.dirs;
    for (const { key, content } of recon.reloads) {
      const id = `file:${key}`;
      docStore = reloadDoc(docStore, id, content);
      loadRequests = {
        ...loadRequests,
        [id]: { content, token: ++loadToken },
      };
    }
    // Strike through any open file whose key the scan dropped (deleted/moved);
    // its buffer stays editable and a Save rewrites it to disk.
    const present = new Set(Object.keys(scan.files));
    docStore = markMissingOnDisk(docStore, (key) => !present.has(key));
    for (const d of docStore.docs)
      if (d.id.startsWith("file:")) compileDoc(d.id);
  }

  // Open (or focus) an entry clicked in the project dock: a saved file opens
  // from the project map, a draft just refocuses its open tab.
  function onOpenEntry(entry: DockEntry) {
    if (entry.path !== null) addOrFocusFile(entry.key);
    else focusDoc(entry.key);
  }

  // A right-click menu pick in the dock. New File/Folder open an inline input in
  // the target folder (or project root); Rename opens it over the row; Delete
  // confirms first. The fs ops themselves land in T7.36 sub-chunks 2.3/2.4 —
  // this chunk wires the menu → inline-edit interaction with stubbed commits.
  function onDockContext(action: string, target: DockTarget) {
    if (action === "new-file" || action === "new-folder") {
      const parentPath = target.kind === "folder" ? target.path : "";
      pendingEdit = { kind: action, parentPath, initial: "" };
    } else if (action === "rename" && target.kind === "folder") {
      pendingEdit = {
        kind: "rename",
        targetKey: target.path,
        isFolder: true,
        initial: basename(target.path),
      };
    } else if (action === "rename" && target.kind === "file") {
      pendingEdit = {
        kind: "rename",
        targetKey: target.key,
        isFolder: false,
        initial: basename(target.path),
      };
    } else if (action === "delete" && target.kind !== "root") {
      void deleteEntry(target);
    }
  }

  // Commit an inline name edit. New File/Folder create against the live folder,
  // then update the map/dirs optimistically and open the new file — the watcher
  // re-scan converges on the same state. (Rename lands in T7.36 2.4.)
  async function commitDockEdit(name: string) {
    const edit = pendingEdit;
    pendingEdit = null;
    const root = projectRoot;
    if (!edit || !root) return;
    if (edit.kind === "rename") {
      await renameEntry(edit.targetKey, edit.isFolder, name, root);
      return;
    }
    const leaf = edit.kind === "new-file" ? withCtabExtension(name) : name;
    const key = edit.parentPath ? `${edit.parentPath}/${leaf}` : leaf;
    const abs = resolvePath(root, key);
    try {
      if (edit.kind === "new-file") {
        // A name collision just focuses the existing file rather than clobbering.
        if (key in projectFiles) return addOrFocusFile(key);
        await createFile(abs, "");
        projectFiles = { ...projectFiles, [key]: "" };
        filePaths = { ...filePaths, [key]: abs };
        addOrFocusFile(key);
      } else {
        await createDir(abs);
        if (!projectDirs.includes(key)) projectDirs = [...projectDirs, key];
      }
    } catch (e) {
      console.error("dock create failed:", e);
      window.alert(`Could not create “${leaf}”: ${(e as Error).message}`);
    }
  }

  function cancelDockEdit() {
    pendingEdit = null;
  }

  // Delete a dock file or folder from the live folder after confirming. Then
  // close any open tabs the delete orphaned (the file is gone, not just missing)
  // and drop its rows optimistically; the watcher re-scan reconciles the rest.
  async function deleteEntry(target: Exclude<DockTarget, { kind: "root" }>) {
    const key = target.path;
    const isFolder = target.kind === "folder";
    const ok = await askConfirm({
      message: isFolder
        ? `Delete folder “${basename(key)}” and everything in it?`
        : `Delete “${basename(key)}”?`,
      confirmLabel: "Delete",
      destructive: true,
    });
    const root = projectRoot;
    if (!ok || !root) return;
    const abs =
      (target.kind === "file" ? filePaths[key] : null) ??
      resolvePath(root, key);
    try {
      await removePath(abs, isFolder);
    } catch (e) {
      console.error("dock delete failed:", e);
      window.alert(
        `Could not delete “${basename(key)}”: ${(e as Error).message}`,
      );
      return;
    }
    // Files removed: the target file, or every file under the deleted folder.
    const gone = isFolder
      ? Object.keys(projectFiles).filter(
          (k) => k === key || k.startsWith(key + "/"),
        )
      : [key];
    for (const k of gone) forceCloseDoc(`file:${k}`);
    projectFiles = omitKeys(projectFiles, gone);
    filePaths = omitKeys(filePaths, gone);
    projectDirs = projectDirs.filter(
      (d) => !(d === key || d.startsWith(key + "/")),
    );
  }

  // Close every open view of a doc and drop its session — no dirty guard (an
  // explicit delete is the user's intent). A no-op if the doc isn't open.
  function forceCloseDoc(docId: string) {
    let ws = workspace;
    const ids = ws.groups
      .flatMap((g) => g.tabs)
      .filter((t) => t.docId === docId)
      .map((t) => t.id);
    if (ids.length === 0) return;
    for (const id of ids) ws = closeTab(ws, id);
    workspace = ws;
    cleanupDoc(docId);
    docStore = removeDoc(docStore, docId);
    reconcileActive();
  }

  // A shallow copy of `map` without the given keys.
  function omitKeys<T>(
    map: Record<string, T>,
    keys: string[],
  ): Record<string, T> {
    const drop = new Set(keys);
    return Object.fromEntries(
      Object.entries(map).filter(([k]) => !drop.has(k)),
    );
  }

  // A key is `base` itself or a descendant of it (`base/...`).
  const isUnder = (key: string, base: string) =>
    key === base || key.startsWith(base + "/");
  // Swap the `oldBase` prefix of `key` for `newBase` (the rename re-key).
  const reprefix = (key: string, oldBase: string, newBase: string) =>
    key === oldBase ? newBase : newBase + key.slice(oldBase.length);

  // Rename a dock file or folder on the live folder, then re-key the project map,
  // paths, and dirs (a folder carries its whole subtree along) and migrate any
  // open file so its tab follows — buffer and dirty state preserved. The watcher
  // re-scan converges on the same keys.
  async function renameEntry(
    oldKey: string,
    isFolder: boolean,
    name: string,
    root: string,
  ) {
    const leaf = isFolder ? name : withCtabExtension(name);
    const slash = oldKey.lastIndexOf("/");
    const newKey = slash >= 0 ? oldKey.slice(0, slash + 1) + leaf : leaf;
    if (newKey === oldKey) return;
    // Don't clobber an existing sibling file/folder.
    if (newKey in projectFiles || projectDirs.includes(newKey)) {
      window.alert(`“${leaf}” already exists.`);
      return;
    }
    const oldAbs =
      (!isFolder ? filePaths[oldKey] : null) ?? resolvePath(root, oldKey);
    try {
      await renamePath(oldAbs, resolvePath(root, newKey));
    } catch (e) {
      console.error("dock rename failed:", e);
      window.alert(`Could not rename to “${leaf}”: ${(e as Error).message}`);
      return;
    }
    // Migrate every open file under the renamed key so its tab follows.
    const affected = Object.keys(projectFiles).filter((k) =>
      isUnder(k, oldKey),
    );
    for (const k of affected) {
      const nk = reprefix(k, oldKey, newKey);
      renameOpenDoc(k, nk, resolvePath(root, nk));
    }
    // Re-key the import map, paths (values become the new abs), and dirs.
    projectFiles = Object.fromEntries(
      Object.entries(projectFiles).map(([k, v]) =>
        isUnder(k, oldKey) ? [reprefix(k, oldKey, newKey), v] : [k, v],
      ),
    );
    filePaths = Object.fromEntries(
      Object.entries(filePaths).map(([k, v]) => {
        if (!isUnder(k, oldKey)) return [k, v];
        const nk = reprefix(k, oldKey, newKey);
        return [nk, resolvePath(root, nk)];
      }),
    );
    projectDirs = projectDirs.map((d) =>
      isUnder(d, oldKey) ? reprefix(d, oldKey, newKey) : d,
    );
  }

  // Move an open file's session + views from `file:<oldKey>` to `file:<newKey>`.
  // The reactive per-doc maps carry over; the live-compiler/edit-handler closures
  // (which captured the old id) are dropped so they recreate under the new id,
  // then the doc recompiles. A no-op if the file isn't open.
  function renameOpenDoc(oldKey: string, newKey: string, newAbs: string) {
    const oldId = `file:${oldKey}`;
    const newId = `file:${newKey}`;
    if (!docFor(oldId)) return;
    results = renameKey(results, oldId, newId);
    errors = renameKey(errors, oldId, newId);
    selections = renameKey(selections, oldId, newId);
    activeSpans = renameKey(activeSpans, oldId, newId);
    layoutWidths = renameKey(layoutWidths, oldId, newId);
    loadRequests = renameKey(loadRequests, oldId, newId);
    delete compilers[oldId];
    delete editHandlers[oldId];
    docStore = renameDocSession(docStore, oldId, newId, {
      name: basename(newKey),
      path: newAbs,
    });
    workspace = renameDocWorkspace(workspace, oldId, newId);
    compileDoc(newId);
  }

  // A shallow copy of `map` with one key renamed (value preserved); unchanged if
  // `oldK` is absent.
  function renameKey<T>(
    map: Record<string, T>,
    oldK: string,
    newK: string,
  ): Record<string, T> {
    if (!(oldK in map)) return map;
    const { [oldK]: value, ...rest } = map;
    return { ...rest, [newK]: value };
  }

  // Save the current score. Overwrites the known path in place; for a never-
  // saved doc, prompts a dialog seeded from the open file's name or the title.
  async function saveFile() {
    // Format-on-save (T7.25): canonicalize the active doc first, then persist the
    // formatted text (using the returned value, not the editor's debounced echo).
    let content = source;
    if (formatOnSave && active) {
      content = (await formatDoc(active.id)) ?? source;
    }
    const saved = await saveDocument(content, {
      path: currentPath,
      suggestedName: currentName ?? defaultDocName(content),
    });
    if (!saved) return;
    docStore = markActiveSaved(docStore, {
      path: saved.path,
      name: saved.name,
    });
  }

  // Export the project as one portable `.ctabz` bundle: every project file plus
  // the live active source under its key. The bundle format names one entry —
  // the active doc (its project key, or a derived name for a draft). A derived
  // artifact like the image exports, so it always prompts for a destination and
  // never rebaselines the document's saved state.
  async function exportBundle() {
    const entry = active?.id.startsWith("file:")
      ? active.id.slice(5)
      : (currentName ?? defaultDocName(source));
    await saveBundle(
      { entry, files: { ...projectFiles, [entry]: source } },
      { path: null, suggestedName: basename(entry) },
    );
  }

  // Export the current render as SVG, or as a PNG raster of that SVG. Exports are
  // derived artifacts, so they always prompt for a destination (path: null). Both
  // live behind the topbar Export menu.
  // A transient export-success flash in the bottom bar (the only feedback that an
  // export landed, since it now writes straight to Downloads without a dialog).
  let exportNotice = $state<string | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  function notifyExport(name: string) {
    exportNotice = `Exported ${name}`;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (exportNotice = null), 3000);
  }

  async function exportSvg() {
    if (!activeResult) return;
    const svg = renderTreeToSvg(activeResult.renderTree);
    notifyExport((await saveSvg(svg, exportName())).name);
  }
  async function exportPng() {
    if (!activeResult) return;
    const blob = await svgToPngBlob(renderTreeToSvg(activeResult.renderTree));
    notifyExport((await savePng(blob, exportName())).name);
  }
  // Export the current document as a paginated, print-ready PDF: paginate it into
  // fixed Letter pages in the core, paint the vector PDF, and save the bytes.
  async function exportPdf() {
    if (!active) return;
    const tree = await paginate(
      source,
      { size: "letter", contentWidth: PDF_CONTENT_WIDTH },
      { basePath: active.path, files: projectFiles },
    );
    const { paginatedTreeToPdf } = await import("./lib/pdf");
    const bytes = await paginatedTreeToPdf(tree);
    notifyExport((await savePdf(bytes, exportName())).name);
  }
  // The base name to seed an export with; the save helpers swap the extension.
  const exportName = () => currentName ?? defaultDocName(source);

  // The topbar Export menu (download ▾): one icon opening an SVG/PNG/PDF choice,
  // dismissed on Escape or a pointer down outside it.
  let exportMenuOpen = $state(false);
  function chooseExport(fn: () => void) {
    exportMenuOpen = false;
    fn();
  }
  $effect(() => {
    if (!exportMenuOpen) return;
    function onPointer(e: PointerEvent) {
      const t = e.target;
      if (t instanceof Element && t.closest(".export-wrap")) return;
      exportMenuOpen = false;
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") exportMenuOpen = false;
    }
    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("keydown", onKey);
    };
  });

  // preventDefault overrides the browser's native page-save / open shortcuts.
  // Cmd/Ctrl+O opens a file/bundle; Cmd/Ctrl+Shift+O opens a folder (desktop —
  // a no-op elsewhere); Cmd/Ctrl+S saves the active file.
  function onIOKey(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
    const key = e.key.toLowerCase();
    if (key === "o" && e.shiftKey) {
      if (!desktop) return;
      void openFolderFlow();
    } else if (key === "o" && !e.shiftKey) void openFile();
    else if (key === "s" && !e.shiftKey) void saveFile();
    else return;
    e.preventDefault();
  }
  $effect(() => {
    window.addEventListener("keydown", onIOKey);
    return () => window.removeEventListener("keydown", onIOKey);
  });

  compileDoc(initialId);
</script>

<main>
  <header class="topbar">
    <div class="brand">
      <h1>cadtab</h1>
      <span
        class="doc-name"
        class:dirty
        use:tooltip={currentName ?? "unsaved document"}
      >
        {currentName ?? "untitled"}{dirty ? " •" : ""}
      </span>
    </div>
    <div class="actions">
      {#if !desktop}
        <!-- File/bundle open lives here on web; on desktop it's Cmd/Ctrl+O and
             (folders) the dock's Open Folder, with the native menu in T7.30. -->
        <button
          class="icon-btn"
          onclick={openFile}
          aria-label="Open"
          use:tooltip={"Open score or project (Cmd/Ctrl+O)"}
        >
          <Icon name="folder_open" size={18} />
        </button>
      {/if}
      <button
        class="icon-btn"
        onclick={saveFile}
        aria-label="Save"
        use:tooltip={"Save score (Cmd/Ctrl+S)"}
      >
        <Icon name="save" size={18} />
      </button>
      <span class="sep" aria-hidden="true"></span>
      <button
        class="icon-btn"
        onclick={openPreview}
        aria-label="Preview"
        use:tooltip={"Open the print preview (final light output)"}
      >
        <Icon name="preview" size={18} />
      </button>
      <div class="export-wrap">
        <button
          class="icon-btn"
          aria-label="Export"
          aria-haspopup="menu"
          aria-expanded={exportMenuOpen}
          use:tooltip={"Export the tab (SVG, PNG, PDF)"}
          onclick={() => (exportMenuOpen = !exportMenuOpen)}
        >
          <Icon name="download" size={18} />
        </button>
        {#if exportMenuOpen}
          <div class="menu" role="menu">
            <button
              class="menu-item"
              role="menuitem"
              onclick={() => chooseExport(exportSvg)}>Export SVG</button
            >
            <button
              class="menu-item"
              role="menuitem"
              onclick={() => chooseExport(exportPng)}>Export PNG</button
            >
            <button
              class="menu-item"
              role="menuitem"
              onclick={() => chooseExport(exportPdf)}>Export PDF</button
            >
            <button
              class="menu-item"
              role="menuitem"
              onclick={() => chooseExport(exportBundle)}
              >Export Bundle (.ctabz)</button
            >
          </div>
        {/if}
      </div>
      <span class="sep" aria-hidden="true"></span>
      <button
        class="icon-btn theme-toggle"
        onclick={cycleTheme}
        aria-label="Theme: {theme}"
        use:tooltip={`Theme: ${theme}`}
      >
        <Icon name={themeIcon(theme)} size={18} />
      </button>
    </div>
  </header>
  <div class="body">
    {#if dockOpen}
      <Dock
        entries={dockEntries}
        dirs={projectDirs}
        {projectName}
        {activeKey}
        canManage={projectRoot !== null}
        {pendingEdit}
        onOpen={onOpenEntry}
        onOpenFolder={desktop ? openFolderFlow : undefined}
        onContext={onDockContext}
        onCommitEdit={commitDockEdit}
        onCancelEdit={cancelDockEdit}
      />
    {/if}
    <Workspace
      bind:workspace
      {missingDocIds}
      {docName}
      onActivateView={focusView}
      onCloseTab={closeView}
      onOpenRender={openRender}
      onNew={newFromTemplate}
      newTemplates={TEMPLATES}
      onFit={fitRender}
    >
      {#snippet view(instance)}
        <!-- Key by instance so switching a group to a different document's tab
             mounts a fresh editor/render for that file (the editor seeds its
             buffer from `doc` only at mount). -->
        {#key instance.id}
          {#if instance.type === "editor"}
            <!-- Pointerdown (not just CodeMirror's focus event) makes the doc
                 active, so active-follows-focus is reliable in WKWebView. -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="editor-pane" onpointerdown={() => focusView(instance)}>
              <Editor
                doc={docFor(instance.docId)?.content ?? ""}
                onChange={onChangeFor(instance.docId ?? "")}
                onCursor={(pos) => handleCursor(instance.docId ?? "", pos)}
                onFocus={() => focusView(instance)}
                zoom={editorZoom}
                selection={selections[instance.docId ?? ""] ?? null}
                loadRequest={loadRequests[instance.docId ?? ""] ?? null}
                formatRequest={formatRequests[instance.docId ?? ""] ?? null}
                tokens={results[instance.docId ?? ""]?.tokens ?? []}
                diagnostics={results[instance.docId ?? ""]?.diagnostics ?? []}
                completions={completionsByDoc[instance.docId ?? ""] ??
                  emptyCompletions}
                {autocomplete}
              />
            </div>
          {:else if instance.type === "render"}
            <RenderView
              result={results[instance.docId ?? ""] ?? null}
              error={errors[instance.docId ?? ""] ?? ""}
              zoom={renderZoom}
              activeSpan={activeSpans[instance.docId ?? ""] ?? null}
              onPrimitiveClick={(span) =>
                handlePrimitiveClick(instance.docId ?? "", span)}
              onClearHighlight={() => clearHighlight(instance.docId ?? "")}
              onReflow={(px) => reflowDoc(instance.docId ?? "", px)}
              onActivate={() => focusView(instance)}
            />
          {:else if instance.type === "preview"}
            {@const previewDoc = docFor(instance.docId ?? "")}
            <PreviewView
              source={previewDoc?.content ?? ""}
              basePath={previewDoc?.path ?? null}
              files={projectFiles}
              error={errors[instance.docId ?? ""] ?? ""}
              onActivate={() => focusView(instance)}
            />
          {/if}
        {/key}
      {/snippet}
    </Workspace>
  </div>
  <BottomBar
    diagnostics={activeResult?.diagnostics ?? []}
    {dockOpen}
    notice={exportNotice}
    {autocomplete}
    {formatOnSave}
    onToggleDock={toggleDock}
    onToggleAutocomplete={toggleAutocomplete}
    onToggleFormatOnSave={toggleFormatOnSave}
  />
  <ConfirmDialog
    open={confirmPrompt !== null}
    message={confirmPrompt?.message ?? ""}
    confirmLabel={confirmPrompt?.confirmLabel ?? "Confirm"}
    cancelLabel={confirmPrompt?.cancelLabel ?? "Cancel"}
    destructive={confirmPrompt?.destructive ?? false}
    onConfirm={() => settleConfirm(true)}
    onCancel={() => settleConfirm(false)}
  />
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    /* The shell is fixed to the viewport and never scrolls — overflow is pushed
       down into the scrollable view bodies (editor, render, preview, dock), each
       of which clips and scrolls on its own. */
    overflow: hidden;
    margin: 0;
    font-family: system-ui, sans-serif;
    background: var(--bg);
    color: var(--fg);
  }
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 1rem;
    border-bottom: 1px solid var(--border);
  }
  .brand {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: 1.1rem;
  }
  .doc-name {
    font-size: 0.85rem;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .doc-name.dirty {
    color: var(--fg);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  /* A thin divider separating file actions from export actions. */
  .sep {
    width: 1px;
    align-self: stretch;
    margin: 0.15rem 0.15rem;
    background: var(--border);
  }
  /* Topbar actions are icon-only square buttons, labelled by tooltip. */
  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.9rem;
    height: 1.9rem;
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    border-radius: 0.3rem;
    cursor: pointer;
    padding: 0;
  }
  .icon-btn:hover {
    background: color-mix(in srgb, var(--fg) 8%, transparent);
  }
  /* The Export control anchors its SVG/PNG menu just below the download icon. */
  .export-wrap {
    position: relative;
    display: flex;
  }
  .menu {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 10;
    margin-top: 0.25rem;
    display: flex;
    flex-direction: column;
    min-width: 9rem;
    padding: 0.25rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    box-shadow: 0 6px 18px color-mix(in srgb, var(--fg) 18%, transparent);
  }
  .menu-item {
    border: none;
    background: transparent;
    color: var(--fg);
    text-align: left;
    padding: 0.35rem 0.55rem;
    border-radius: 0.25rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.82rem;
    white-space: nowrap;
  }
  .menu-item:hover {
    background: color-mix(in srgb, var(--fg) 10%, transparent);
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .editor-pane {
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
</style>
