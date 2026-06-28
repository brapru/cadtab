<script lang="ts">
  import { renderTreeToSvg } from "./svg";
  import type { CompileResult } from "./types";

  // Print preview (T7.6): the final printed output of a document — the same
  // light, self-contained SVG the export produces (T5.3), shown inline. Reuses
  // the live render tree via the export serializer, so it is never a second
  // layout pipeline; always light, regardless of the app theme.
  let {
    result = null,
    error = "",
    onActivate,
  }: {
    result?: CompileResult | null;
    error?: string;
    onActivate?: () => void;
  } = $props();

  const svg = $derived(result ? renderTreeToSvg(result.renderTree) : "");
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="preview" onpointerdown={() => onActivate?.()}>
  {#if result}
    <div class="sheet">
      <!-- Our own serializer output (text escaped in svg.ts), not user HTML. -->
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html svg}
    </div>
  {:else if error}
    <p class="error">{error}</p>
  {/if}
</div>

<style>
  /* The page (.sheet) stays white — it's the printed output — but the surrounding
     backdrop tracks the theme: a light gray in light mode (~#d9d9d9), a dark gray
     in dark mode, so the sheet isn't a harsh bright panel against a dark UI. */
  .preview {
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow: auto;
    padding: 1.5rem;
    background: color-mix(in srgb, var(--fg) 15%, var(--bg));
    display: flex;
    justify-content: center;
    align-items: flex-start;
  }
  .sheet {
    background: #ffffff;
    box-shadow: 0 1px 6px rgba(0, 0, 0, 0.3);
    max-width: 100%;
  }
  /* Scale the exported SVG to the page width while keeping its aspect ratio. */
  .sheet :global(svg) {
    display: block;
    width: 100%;
    height: auto;
  }
  .error {
    color: var(--fg);
    opacity: 0.7;
    padding: 1rem;
  }
</style>
