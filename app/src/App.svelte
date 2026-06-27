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
  import type { CompileResult, Span } from "./lib/types";

  const initialDoc = "score {\n  3:0 2:0 1:0 5:0\n}\n";

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
    void live.run(src, { width: layoutWidth });
  }

  const onChange = debounce((value: string) => recompile(value), 150);

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

  recompile(initialDoc);
</script>

<main>
  <header class="topbar">
    <h1>cadtab</h1>
    <button
      class="theme-toggle"
      onclick={cycleTheme}
      aria-label="Theme: {theme}"
      title="Theme: {theme}">{themeGlyph(theme)}</button
    >
  </header>
  <div class="panes" bind:this={panesEl}>
    <div class="editor-pane" style="flex: {splitRatio}">
      <Editor
        doc={initialDoc}
        {onChange}
        onCursor={handleCursor}
        {selection}
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
  h1 {
    margin: 0;
    font-size: 1.1rem;
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
