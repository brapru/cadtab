<script lang="ts">
  import Editor from "./lib/Editor.svelte";
  import Tab from "./lib/Tab.svelte";
  import { compile } from "./lib/core";
  import { createLiveCompiler } from "./lib/live";
  import { debounce } from "./lib/debounce";
  import { byteToCharIndex, charToByteIndex, spanToRange } from "./lib/spans";
  import { narrowestSpanAt } from "./lib/mapping";
  import { clampSplit, splitFromPointer } from "./lib/split";
  import { layoutWidthForPx, clampZoom, ZOOM_STEP } from "./lib/sizing";
  import { nextTheme, themeGlyph, type Theme } from "./lib/theme";
  import {
    openProject,
    saveDocument,
    saveBundle,
    saveSvg,
    savePng,
    defaultDocName,
  } from "./lib/io";
  import { renderTreeToSvg } from "./lib/svg";
  import { svgToPngBlob } from "./lib/png";
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

  let result = $state<CompileResult | null>(null);
  let error = $state("");
  // The source the current result was compiled from, so cursor<->span conversions
  // line up with the spans in that render tree.
  let source = $state(initialDoc);
  let selection = $state<{ from: number; to: number } | null>(null);
  let activeSpan = $state<Span | null>(null);
  // Layout width (logical units) and the measured render-pane width that drives
  // it; reflow re-lays-out when the pane resizes.
  let layoutWidth = $state(66);
  let paneWidth = $state(0);

  const live = createLiveCompiler(
    compile,
    (r) => {
      result = r;
      error = "";
    },
    () => {
      error = "core unavailable (no backend)";
    },
  );

  function recompile(src: string) {
    source = src;
    // Import context: desktop resolves beside the open file (basePath) and from
    // the bundle map; web resolves from the bundle map alone.
    void live.run(
      src,
      { width: layoutWidth },
      { basePath: currentPath, files: projectFiles },
    );
  }

  // Document session: the name we last opened/saved as, and whether there are
  // unsaved edits. Tabs/dirty-per-doc arrive with the M7 dock; for now it is a
  // single in-place document.
  let currentName = $state<string | null>(null);
  // The open score's standalone path (desktop); null for the default doc, on
  // web, or when the score lives inside an opened bundle. A known path lets Save
  // overwrite in place instead of re-prompting.
  let currentPath = $state<string | null>(null);
  // The project's importable libs (path -> contents), backing import resolution
  // on web; populated when a `.ctabz` bundle is opened. The entry score is the
  // editor buffer, not in here.
  let projectFiles = $state<Record<string, string>>({});
  // The opened/saved bundle's path (desktop), for in-place "Save Project".
  let bundlePath = $state<string | null>(null);
  let dirty = $state(false);
  // The text as of the last open/save: the baseline the dirty flag compares
  // against, so editing then undoing back to it clears dirty (and a programmatic
  // load, which echoes the baseline, never reads as an edit).
  let savedContent = initialDoc;
  // A versioned signal that pushes opened content into the editor.
  let loadRequest = $state<{ content: string; token: number } | null>(null);
  let loadToken = 0;

  // Dirty iff the document now differs from the last saved/opened text.
  function handleEdit(value: string) {
    dirty = value !== savedContent;
    recompile(value);
  }

  const onChange = debounce((value: string) => handleEdit(value), 150);

  // Reflow: when the render pane settles at a new width, re-lay-out the current
  // source at the matching logical width (debounced per resize tick).
  const reflow = debounce((px: number) => {
    layoutWidth = layoutWidthForPx(px);
    recompile(source);
  }, 150);
  $effect(() => {
    if (paneWidth > 0) reflow(paneWidth);
  });

  // Render -> source: a clicked primitive selects its source range in the editor.
  function handlePrimitiveClick(span: Span) {
    const range = spanToRange(byteToCharIndex(source), span);
    if (range) selection = range;
  }

  // Source -> render: the cursor lights up the primitive(s) sharing its range.
  function handleCursor(pos: number) {
    if (!result) return;
    const byte = charToByteIndex(source)[pos] ?? 0;
    activeSpan = narrowestSpanAt(result.renderTree, byte);
  }

  // Clicking empty render space (or Escape) drops the highlight, mirroring how
  // clicking off a note in the editor clears it. Primitive clicks stop
  // propagating, so this only fires for the background.
  function clearHighlight() {
    activeSpan = null;
  }

  // Draggable split between the editor and render panes; editor takes
  // `splitRatio` of the width, render the rest. Arrow keys nudge it for keyboard
  // users.
  let panesEl: HTMLDivElement;
  let splitRatio = $state(0.5);
  let dragging = $state(false);

  function startDrag(e: PointerEvent) {
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }
  function onDrag(e: PointerEvent) {
    if (!dragging) return;
    splitRatio = splitFromPointer(e.clientX, panesEl.getBoundingClientRect());
  }
  function endDrag(e: PointerEvent) {
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
  }
  function onGutterKey(e: KeyboardEvent) {
    if (e.key === "ArrowLeft") splitRatio = clampSplit(splitRatio - 0.02);
    else if (e.key === "ArrowRight") splitRatio = clampSplit(splitRatio + 0.02);
  }

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

  // Make `content` the open document: reset the dirty baseline, swap the editor
  // buffer (fresh history), and render. `path`/`bundlePath` track where Save and
  // Save Project write; `libs` are the importable files for the provider.
  function loadDocument(opts: {
    path: string | null;
    bundlePath: string | null;
    name: string;
    content: string;
    libs: Record<string, string>;
  }) {
    savedContent = opts.content;
    currentPath = opts.path;
    bundlePath = opts.bundlePath;
    currentName = opts.name;
    projectFiles = opts.libs;
    dirty = false;
    loadRequest = { content: opts.content, token: ++loadToken };
    recompile(opts.content);
  }

  // Open a score (`.ctab`) or a whole project bundle (`.ctabz`), guarding unsaved
  // edits first. A bundle loads its entry into the editor and its other files as
  // importable libs; a single score loads with no libs.
  async function openFile() {
    if (dirty && !window.confirm("Discard unsaved changes?")) return;
    let opened;
    try {
      opened = await openProject();
    } catch (e) {
      window.alert(`Could not open project: ${(e as Error).message}`);
      return;
    }
    if (!opened) return;

    if (opened.kind === "single") {
      loadDocument({
        path: opened.path,
        bundlePath: null,
        name: opened.name,
        content: opened.content,
        libs: {},
      });
      return;
    }
    const { entry, files } = opened.bundle;
    const libs = { ...files };
    delete libs[entry];
    loadDocument({
      path: null,
      bundlePath: opened.path,
      name: entry,
      content: files[entry],
      libs,
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
    savedContent = source;
    currentPath = saved.path;
    currentName = saved.name;
    dirty = false;
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
    savedContent = source;
    dirty = false;
  }

  // Export the current render as SVG, or as a PNG raster of that SVG. Exports are
  // derived artifacts, so they always prompt for a destination (path: null).
  async function exportSvg() {
    if (!result) return;
    const svg = renderTreeToSvg(result.renderTree);
    await saveSvg(svg, { path: null, suggestedName: exportName() });
  }
  async function exportPng() {
    if (!result) return;
    const blob = await svgToPngBlob(renderTreeToSvg(result.renderTree));
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

  recompile(initialDoc);
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
      <button onclick={openFile} title="Open score or project (Cmd/Ctrl+O)"
        >Open</button
      >
      <button onclick={saveFile} title="Save score (Cmd/Ctrl+S)">Save</button>
      <button
        onclick={saveProject}
        title="Save project bundle (Cmd/Ctrl+Shift+S)">Save Project</button
      >
      <span class="sep" aria-hidden="true"></span>
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
  <div class="panes" bind:this={panesEl}>
    <div class="editor-pane" style="flex: {splitRatio}">
      <Editor
        doc={initialDoc}
        {onChange}
        onCursor={handleCursor}
        {selection}
        {loadRequest}
        tokens={result?.tokens ?? []}
        diagnostics={result?.diagnostics ?? []}
      />
    </div>
    <!-- The splitter is a slider over the editor's share of the width: drag it,
         or use the arrow keys when focused. -->
    <div
      class="gutter"
      class:dragging
      role="slider"
      aria-label="Resize editor and render panes"
      aria-valuemin={15}
      aria-valuemax={85}
      aria-valuenow={Math.round(splitRatio * 100)}
      tabindex="0"
      onpointerdown={startDrag}
      onpointermove={onDrag}
      onpointerup={endDrag}
      onkeydown={onGutterKey}
    ></div>
    <div class="render-side" style="flex: {1 - splitRatio}">
      <div class="render-toolbar">
        <button onclick={zoomOut} aria-label="Zoom out">−</button>
        <span class="zoom-level">{Math.round(zoom * 100)}%</span>
        <button onclick={zoomIn} aria-label="Zoom in">+</button>
        <button onclick={zoomFit} aria-label="Fit to width">Fit</button>
      </div>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="render-pane"
        bind:clientWidth={paneWidth}
        onclick={clearHighlight}
        onkeydown={(e) => e.key === "Escape" && clearHighlight()}
      >
        {#if result}
          <Tab
            tree={result.renderTree}
            {zoom}
            {activeSpan}
            onPrimitiveClick={handlePrimitiveClick}
          />
        {:else if error}
          <p class="error">{error}</p>
        {/if}
      </div>
    </div>
  </div>
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
  .actions button {
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    border-radius: 0.3rem;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
    font-size: 0.85rem;
    line-height: 1;
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
  .panes {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .editor-pane {
    min-width: 0;
  }
  .render-side {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .render-toolbar {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.5rem;
    border-bottom: 1px solid var(--border);
  }
  .render-toolbar button {
    min-width: 1.8rem;
    padding: 0.1rem 0.4rem;
    cursor: pointer;
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    border-radius: 0.25rem;
  }
  .zoom-level {
    min-width: 3rem;
    text-align: center;
    font-variant-numeric: tabular-nums;
    font-size: 0.85rem;
  }
  .render-pane {
    flex: 1;
    padding: 1rem;
    overflow: auto;
    min-width: 0;
  }
  /* Draggable divider between the two panes. */
  .gutter {
    flex: 0 0 6px;
    cursor: col-resize;
    background: var(--border);
    touch-action: none;
  }
  .gutter:hover,
  .gutter.dragging,
  .gutter:focus-visible {
    background: color-mix(in srgb, var(--fg) 35%, transparent);
    outline: none;
  }
  .error {
    opacity: 0.7;
  }
</style>
