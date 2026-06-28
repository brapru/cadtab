<script lang="ts">
  import Tab from "./Tab.svelte";
  import { debounce } from "./debounce";
  import type { CompileResult, Span } from "./types";

  // One document's render, as a placeable view (T7.5/T7.4b). It owns its own pane
  // width and reflow so several renders can coexist at different sizes; zoom is a
  // shared app-level control passed in. The doc-bound wiring (which file this
  // renders) lives in the parent's snippet.
  let {
    result = null,
    error = "",
    zoom = 1,
    activeSpan = null,
    onPrimitiveClick,
    onClearHighlight,
    onReflow,
    onZoomIn,
    onZoomOut,
    onZoomFit,
    onActivate,
  }: {
    result?: CompileResult | null;
    error?: string;
    zoom?: number;
    activeSpan?: Span | null;
    onPrimitiveClick?: (span: Span) => void;
    onClearHighlight?: () => void;
    onReflow?: (px: number) => void;
    onZoomIn?: () => void;
    onZoomOut?: () => void;
    onZoomFit?: () => void;
    onActivate?: () => void;
  } = $props();

  // Re-lay-out this doc when its pane settles at a new width (debounced).
  let paneWidth = $state(0);
  const reflow = debounce((px: number) => onReflow?.(px), 150);
  $effect(() => {
    if (paneWidth > 0) reflow(paneWidth);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="render-side" onpointerdown={() => onActivate?.()}>
  <div class="render-toolbar">
    <button onclick={() => onZoomOut?.()} aria-label="Zoom out">−</button>
    <span class="zoom-level">{Math.round(zoom * 100)}%</span>
    <button onclick={() => onZoomIn?.()} aria-label="Zoom in">+</button>
    <button onclick={() => onZoomFit?.()} aria-label="Fit to width">Fit</button>
  </div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="render-pane"
    bind:clientWidth={paneWidth}
    onclick={() => onClearHighlight?.()}
    onkeydown={(e) => e.key === "Escape" && onClearHighlight?.()}
  >
    {#if result}
      <Tab
        tree={result.renderTree}
        {zoom}
        {activeSpan}
        onPrimitiveClick={(s) => onPrimitiveClick?.(s)}
      />
    {:else if error}
      <p class="error">{error}</p>
    {/if}
  </div>
</div>

<style>
  .render-side {
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
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
  .error {
    opacity: 0.7;
  }
</style>
