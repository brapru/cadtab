<script lang="ts">
  import Tab from "./Tab.svelte";
  import { debounce } from "./debounce";
  import type { CompileResult, Span } from "./types";

  // One document's render, as a placeable view. It owns its own pane
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
    onActivate,
  }: {
    result?: CompileResult | null;
    error?: string;
    zoom?: number;
    activeSpan?: Span | null;
    onPrimitiveClick?: (span: Span) => void;
    onClearHighlight?: () => void;
    onReflow?: (px: number) => void;
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
  <!-- Zoom lives on Cmd/Ctrl +/- and the tab-strip Fit control. -->
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
  .render-pane {
    flex: 1;
    /* min-height: 0 lets this shrink within the column so its own overflow:auto
       engages on a tall render — without it the pane grows to content height and
       pushes the shell instead of scrolling internally. */
    min-height: 0;
    padding: 1rem;
    overflow: auto;
    min-width: 0;
  }
  .error {
    opacity: 0.7;
  }
</style>
