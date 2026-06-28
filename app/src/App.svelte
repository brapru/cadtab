<script lang="ts">
  import Editor from "./lib/Editor.svelte";
  import RenderView from "./lib/RenderView.svelte";
  import PreviewView from "./lib/PreviewView.svelte";
  import Workspace from "./lib/Workspace.svelte";
  import BottomBar from "./lib/BottomBar.svelte";
  import Dock from "./lib/Dock.svelte";
  import ConfirmDialog from "./lib/ConfirmDialog.svelte";
  import { compile } from "./lib/core";
  import { createLiveCompiler } from "./lib/live";
  import { debounce } from "./lib/debounce";
  import { byteToCharIndex, charToByteIndex, spanToRange } from "./lib/spans";
  import { narrowestSpanAt } from "./lib/mapping";
  import {
    defaultWorkspace,
    instance as viewInstance,
    addTab,
    groupOfType,
    type Workspace as WorkspaceModel,
  } from "./lib/workspace";
  import {
    newSession,
    singleDocStore,
    activeDoc,
    isDirty,
    putDoc,
    setDocContent,
    setActive,
    markActiveSaved,
    type DocStore,
    type DocSession,
  } from "./lib/documents";
  import { layoutWidthForPx, clampZoom, ZOOM_STEP } from "./lib/sizing";
  import { nextTheme, themeGlyph, type Theme } from "./lib/theme";
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
      workspace = addDocTabs(workspace, o.id);
    }
    focusDoc(o.id);
    compileDoc(o.id);
  }

  // Open (or focus) the active document's print preview as a tab beside its
  // render. The preview reuses that doc's compile result, so no extra compile.
  function openPreview() {
    if (!active) return;
    const group =
      groupOfType(workspace, "render") ??
      groupOfType(workspace, "editor") ??
      workspace.groups[0]?.id;
    if (group) {
      workspace = addTab(workspace, viewInstance("preview", active.id), group);
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

  // Visual zoom of the render. Fit returns to 1, which fills the pane width since
  // layout already reflows to it.
  let zoom = $state(1);
  const zoomIn = () => (zoom = clampZoom(zoom * ZOOM_STEP));
  const zoomOut = () => (zoom = clampZoom(zoom / ZOOM_STEP));
  const zoomFit = () => (zoom = 1);

  // Cmd/Ctrl +/- zoom the render and Cmd/Ctrl 0 fits, overriding the browser's
  // native page zoom. `=`/`+` share a key (shift), as do `-`/`_`.
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
  // current project context. The `<select>` resets to its placeholder after each
  // pick. No discard guard: opening never replaces the edited doc, it adds a tab.
  let newChoice = $state("");
  function onNewSelect() {
    const id = newChoice;
    newChoice = "";
    const template = id ? templateById(id) : undefined;
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
  // derived artifacts, so they always prompt for a destination (path: null).
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
        title={currentName ?? "unsaved document"}
      >
        {currentName ?? "untitled"}{dirty ? " •" : ""}
      </span>
    </div>
    <div class="actions">
      <select
        class="new-select"
        aria-label="New from template"
        bind:value={newChoice}
        onchange={onNewSelect}
      >
        <option value="" disabled>New…</option>
        {#each TEMPLATES as t (t.id)}
          <option value={t.id}>{t.label}</option>
        {/each}
      </select>
      <button onclick={openFile} title="Open score or project (Cmd/Ctrl+O)"
        >Open</button
      >
      <button onclick={saveFile} title="Save score (Cmd/Ctrl+S)">Save</button>
      <button
        onclick={saveProject}
        title="Save project bundle (Cmd/Ctrl+Shift+S)">Save Project</button
      >
      <span class="sep" aria-hidden="true"></span>
      <button
        onclick={openPreview}
        title="Open the print preview (final light output)">Preview</button
      >
      <button onclick={exportSvg} title="Export the tab as an SVG image"
        >Export SVG</button
      >
      <button onclick={exportPng} title="Export the tab as a PNG image"
        >Export PNG</button
      >
      <button
        class="theme-toggle"
        onclick={cycleTheme}
        aria-label="Theme: {theme}"
        title="Theme: {theme}">{themeGlyph(theme)}</button
      >
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
      onActivateView={(inst) => inst.docId && focusDoc(inst.docId)}
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
            <div
              class="editor-pane"
              onpointerdown={() => instance.docId && focusDoc(instance.docId)}
            >
              <Editor
                doc={docFor(instance.docId)?.content ?? ""}
                onChange={onChangeFor(instance.docId ?? "")}
                onCursor={(pos) => handleCursor(instance.docId ?? "", pos)}
                onFocus={() => instance.docId && focusDoc(instance.docId)}
                selection={selections[instance.docId ?? ""] ?? null}
                tokens={results[instance.docId ?? ""]?.tokens ?? []}
                diagnostics={results[instance.docId ?? ""]?.diagnostics ?? []}
              />
            </div>
          {:else if instance.type === "render"}
            <RenderView
              result={results[instance.docId ?? ""] ?? null}
              error={errors[instance.docId ?? ""] ?? ""}
              {zoom}
              activeSpan={activeSpans[instance.docId ?? ""] ?? null}
              onPrimitiveClick={(span) =>
                handlePrimitiveClick(instance.docId ?? "", span)}
              onClearHighlight={() => clearHighlight(instance.docId ?? "")}
              onReflow={(px) => reflowDoc(instance.docId ?? "", px)}
              onActivate={() => instance.docId && focusDoc(instance.docId)}
              onZoomIn={zoomIn}
              onZoomOut={zoomOut}
              onZoomFit={zoomFit}
            />
          {:else if instance.type === "preview"}
            <PreviewView
              result={results[instance.docId ?? ""] ?? null}
              error={errors[instance.docId ?? ""] ?? ""}
              onActivate={() => instance.docId && focusDoc(instance.docId)}
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
  .actions button,
  .new-select {
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    border-radius: 0.3rem;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
    font-size: 0.85rem;
    line-height: 1;
  }
  .new-select {
    font-family: inherit;
  }
  .theme-toggle {
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    border-radius: 0.3rem;
    width: 1.9rem;
    height: 1.9rem;
    cursor: pointer;
    font-size: 1rem;
    line-height: 1;
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
