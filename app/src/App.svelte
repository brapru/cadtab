<script lang="ts">
  import Editor from "./lib/Editor.svelte";
  import RenderView from "./lib/RenderView.svelte";
  import PreviewView from "./lib/PreviewView.svelte";
  import Workspace from "./lib/Workspace.svelte";
  import BottomBar from "./lib/BottomBar.svelte";
  import Dock from "./lib/Dock.svelte";
  import ConfirmDialog from "./lib/ConfirmDialog.svelte";
  import Icon from "./lib/Icon.svelte";
  import { compile } from "./lib/core";
  import { createLiveCompiler } from "./lib/live";
  import { debounce } from "./lib/debounce";
  import { byteToCharIndex, charToByteIndex, spanToRange } from "./lib/spans";
  import { narrowestSpanAt } from "./lib/mapping";
  import {
    defaultWorkspace,
    instance as viewInstance,
    addTab,
    closeTab,
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
    setDocContent,
    setActive,
    markActiveSaved,
    type DocStore,
    type DocSession,
  } from "./lib/documents";
  import { layoutWidthForPx, clampZoom, ZOOM_STEP } from "./lib/sizing";
  import { nextTheme, themeIcon, type Theme } from "./lib/theme";
  import {
    openProject,
    saveDocument,
    saveBundle,
    saveSvg,
    savePng,
    defaultDocName,
    basename,
  } from "./lib/io";
  import { renderTreeToSvg } from "./lib/svg";
  import { svgToPngBlob } from "./lib/png";
  import { tooltip } from "./lib/tooltip";
  import { TEMPLATES, templateById } from "./lib/templates";
  import type { CompileResult, Span } from "./lib/types";

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
  let errors = $state<Record<string, string>>({});
  let selections = $state<Record<string, { from: number; to: number } | null>>(
    {},
  );
  let activeSpans = $state<Record<string, Span | null>>({});
  let layoutWidths = $state<Record<string, number>>({});

  // The open documents (T7.4b): each opened/imported file gets its own id, editor
  // tab, and render. The active doc drives the topbar name, Save/Export, and the
  // dirty indicator.
  const initialId = "doc";
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

  // The project context shared by every open document's compile: importable libs
  // (path -> contents) and the bundle path for "Save Project". A stable project
  // entry name heads the dock independent of which doc is focused.
  let projectFiles = $state<Record<string, string>>({});
  let bundlePath = $state<string | null>(null);
  let projectEntryName = $state("untitled");
  // The dock path of the active doc, so the dock marks the focused file.
  const activeDockPath = $derived(
    active && active.id.startsWith("lib:")
      ? active.id.slice(4)
      : (currentName ?? null),
  );

  function docFor(id: string | null): DocSession | undefined {
    return id ? docStore.docs.find((d) => d.id === id) : undefined;
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

  // Compile one document at its own pane width and the shared project context.
  function compileDoc(id: string) {
    const doc = docFor(id);
    if (!doc) return;
    void compilerFor(id).run(
      doc.content,
      { width: layoutWidths[id] ?? 66 },
      { basePath: doc.path, files: projectFiles },
    );
  }

  // Editing updates that document's buffer (dirty derives) and recompiles it. A
  // lib edit also updates the shared project map and recompiles the other open
  // docs that may import it.
  function handleEdit(id: string, value: string) {
    docStore = setDocContent(docStore, id, value);
    if (id.startsWith("lib:")) {
      const libPath = id.slice(4);
      projectFiles = { ...projectFiles, [libPath]: value };
      for (const d of docStore.docs) if (d.id !== id) compileDoc(d.id);
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
  // thing — the editor's code font vs. the render's scale (T7.12).
  let focusedKind = $state<string>("editor");
  function focusView(inst: ViewInstance) {
    if (inst.docId) focusDoc(inst.docId);
    focusedKind = inst.type;
  }

  // The workspace layout (D41): the active doc's editor|render split. Opening a
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
    errors = {};
    selections = {};
    activeSpans = {};
    layoutWidths = {};
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
    errors = without(errors, id);
    selections = without(selections, id);
    activeSpans = without(activeSpans, id);
    layoutWidths = without(layoutWidths, id);
    delete compilers[id];
    delete editHandlers[id];
  }

  // Open or focus a document. Opening a *project* (a single score or a bundle
  // from disk, which supplies `context`) replaces the prior one: it closes the
  // old project's docs, tabs, and renders and resets the import context, so no
  // stale render lingers. Files opened *within* a project — dock-opened libs and
  // New-from-template — omit `context` and just add or focus a tab.
  function openDoc(o: {
    id: string;
    name: string | null;
    path: string | null;
    content: string;
    context?: { libs: Record<string, string>; bundlePath: string | null };
  }) {
    if (o.context) {
      projectFiles = o.context.libs;
      bundlePath = o.context.bundlePath;
      projectEntryName = o.name ?? "untitled";
      resetDocState();
      docStore = singleDocStore(
        newSession(o.id, { name: o.name, path: o.path, content: o.content }),
      );
      workspace = defaultWorkspace(o.id);
      compileDoc(o.id);
      return;
    }
    if (!docStore.docs.some((d) => d.id === o.id)) {
      docStore = putDoc(
        docStore,
        newSession(o.id, { name: o.name, path: o.path, content: o.content }),
      );
      // Reseed a fresh editor|render layout when every tab was closed; otherwise
      // add this doc's tabs beside the open ones.
      workspace =
        workspace.groups.length === 0
          ? defaultWorkspace(o.id)
          : addDocTabs(workspace, o.id);
    }
    focusDoc(o.id);
    compileDoc(o.id);
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

  // Reopen a document's render from its editor tab's launcher (T7.12) — closes
  // the T7.11 gap where a closed render had no way back.
  function openRender(docId: string) {
    openViewFor(docId, "render");
  }

  // Close a tab (T7.11). Each view closes on its own — removing just that
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

  // Project dock visibility, toggled from the bottom bar and Cmd/Ctrl-B. The dock
  // panel it reveals lands in T7.2; the bottom bar already owns the control.
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

  // Zoom is per view type (T7.12): the editor's code font and the render's scale
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

  // Theme: "system" follows the OS; light/dark force a mode via a root attribute
  // the semantic CSS tokens key off.
  let theme = $state<Theme>("system");
  const cycleTheme = () => (theme = nextTheme(theme));
  $effect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
  });

  // Start a new untitled document from a starter template as its own tab in the
  // current project context — driven by the tab-strip New ("+") menu (T7.12). No
  // discard guard: opening never replaces the edited doc, it adds a tab.
  function newFromTemplate(id: string) {
    const template = templateById(id);
    if (!template) return;
    openDoc({
      id: `untitled-${++untitledCount}`,
      name: null,
      path: null,
      content: template.source,
    });
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

  // Opening a project replaces the current one (T7.8), which can discard unsaved
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
      openDoc({
        id: opened.path ?? `web:${opened.name}`,
        name: opened.name,
        path: opened.path,
        content: opened.content,
        context: { libs: {}, bundlePath: null },
      });
      return;
    }
    const { entry, files } = opened.bundle;
    const libs = { ...files };
    delete libs[entry];
    openDoc({
      id: `entry:${entry}`,
      name: entry,
      path: null,
      content: files[entry],
      context: { libs, bundlePath: opened.path },
    });
  }

  // Open (or focus) a file clicked in the project dock. The entry row is the
  // active project document already; a lib row opens from the project map.
  function openDockFile(path: string, isEntry: boolean) {
    if (isEntry) return;
    openDoc({
      id: `lib:${path}`,
      name: basename(path),
      path: null,
      content: projectFiles[path] ?? "",
    });
  }

  // Save the current score. Overwrites the known path in place; for a never-
  // saved doc, prompts a dialog seeded from the open file's name or the title.
  async function saveFile() {
    const saved = await saveDocument(source, {
      path: currentPath,
      suggestedName: currentName ?? defaultDocName(source),
    });
    if (!saved) return;
    docStore = markActiveSaved(docStore, {
      path: saved.path,
      name: saved.name,
    });
  }

  // Save the whole project as one `.ctabz` bundle: the importable libs plus the
  // live entry source. Overwrites the known bundle path in place, else prompts.
  async function saveProject() {
    const entry = currentName ?? defaultDocName(source);
    const saved = await saveBundle(
      { entry, files: { ...projectFiles, [entry]: source } },
      { path: bundlePath, suggestedName: entry },
    );
    if (!saved) return;
    bundlePath = saved.path;
    // The bundle write rebaselines the entry doc without changing its own path.
    docStore = markActiveSaved(docStore, {
      path: currentPath,
      name: currentName,
    });
  }

  // Export the current render as SVG, or as a PNG raster of that SVG. Exports are
  // derived artifacts, so they always prompt for a destination (path: null). Both
  // live behind the topbar Export menu (T7.14).
  async function exportSvg() {
    if (!activeResult) return;
    const svg = renderTreeToSvg(activeResult.renderTree);
    await saveSvg(svg, { path: null, suggestedName: exportName() });
  }
  async function exportPng() {
    if (!activeResult) return;
    const blob = await svgToPngBlob(renderTreeToSvg(activeResult.renderTree));
    await savePng(blob, { path: null, suggestedName: exportName() });
  }
  // The base name to seed an export with; `saveSvg`/`savePng` swap the extension.
  const exportName = () => currentName ?? defaultDocName(source);

  // The topbar Export menu (download ▾): one icon opening an SVG/PNG choice,
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

  // Cmd/Ctrl+O opens, Cmd/Ctrl+S saves, Cmd/Ctrl+Shift+S saves the project;
  // preventDefault overrides the browser's native page-save / open shortcuts.
  function onIOKey(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
    const key = e.key.toLowerCase();
    if (key === "o" && !e.shiftKey) void openFile();
    else if (key === "s" && e.shiftKey) void saveProject();
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
      <button
        class="icon-btn"
        onclick={openFile}
        aria-label="Open"
        use:tooltip={"Open score or project (Cmd/Ctrl+O)"}
      >
        <Icon name="folder_open" size={18} />
      </button>
      <button
        class="icon-btn"
        onclick={saveFile}
        aria-label="Save"
        use:tooltip={"Save score (Cmd/Ctrl+S)"}
      >
        <Icon name="save" size={18} />
      </button>
      <button
        class="icon-btn"
        onclick={saveProject}
        aria-label="Save Project"
        use:tooltip={"Save project bundle (Cmd/Ctrl+Shift+S)"}
      >
        <Icon name="save_as" size={18} />
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
          use:tooltip={"Export the tab as an image"}
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
        entryName={projectEntryName}
        libs={projectFiles}
        projectName={bundlePath ? basename(bundlePath) : "Project"}
        activePath={activeDockPath}
        onOpenFile={openDockFile}
      />
    {/if}
    <Workspace
      bind:workspace
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
                tokens={results[instance.docId ?? ""]?.tokens ?? []}
                diagnostics={results[instance.docId ?? ""]?.diagnostics ?? []}
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
            <PreviewView
              result={results[instance.docId ?? ""] ?? null}
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
    onToggleDock={toggleDock}
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
  /* Topbar actions are icon-only square buttons (T7.14), labelled by tooltip. */
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
