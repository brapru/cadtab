<script lang="ts">
  import type { RenderTree, Primitive, TextRole, Span } from "./types";
  import { spansOverlap } from "./mapping";

  // The painter is thin: it positions primitives verbatim in the layout's
  // logical coordinate space (1 unit = string spacing) and lets the SVG viewBox
  // scale them to pixels. `zoom` multiplies the fit-to-container width.
  // `activeSpan` lights up the primitives that share the source range under the
  // editor cursor; clicking a primitive reports its span back to the editor.
  let {
    tree,
    zoom = 1,
    activeSpan = null,
    onPrimitiveClick,
  }: {
    tree: RenderTree;
    zoom?: number;
    activeSpan?: Span | null;
    onPrimitiveClick?: (span: Span) => void;
  } = $props();

  const isActive = (span: Span | null): boolean =>
    !!span && !!activeSpan && spansOverlap(span, activeSpan);

  // Stop the click from reaching the background handler that clears the
  // highlight, so selecting a primitive does not immediately deselect it.
  function onPrimitiveSelect(e: MouseEvent, span: Span) {
    e.stopPropagation();
    onPrimitiveClick?.(span);
  }

  function onPrimitiveKey(e: KeyboardEvent, span: Span) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onPrimitiveClick?.(span);
    }
  }

  // Per-role text metrics, in logical units, tuned to the row heights the layout
  // engine reserves for header rows so glyphs sit inside their allotted space.
  // Intent lives in the role; geometry stays in the coordinates.
  type TextStyle = { size: number; weight?: number; italic?: boolean };
  const TEXT_STYLE: Record<TextRole, TextStyle> = {
    title: { size: 1.5, weight: 600 },
    composer: { size: 1.0, weight: 700 },
    tuningName: { size: 0.85 },
    tuningString: { size: 0.85 },
    tempo: { size: 0.9, weight: 700 },
    capo: { size: 0.85 },
    fretNumber: { size: 1.3 },
    stringLabel: { size: 1.1 },
    timeSig: { size: 1.4, weight: 600 },
    finger: { size: 0.95 },
    strum: { size: 1.5 },
    technique: { size: 0.95, italic: true },
    ending: { size: 0.95 },
    rest: { size: 1.5 },
  };
</script>

{#snippet drawPrimitive(prim: Primitive)}
  {#if prim.kind === "line"}
    <!-- Butt caps so thick beams/flags end exactly at their endpoints rather
         than bulging past the outer stems with rounded blobs. -->
    <line
      x1={prim.x1}
      y1={prim.y1}
      x2={prim.x2}
      y2={prim.y2}
      stroke-width={prim.weight}
      stroke-linecap="butt"
    />
  {:else if prim.kind === "text"}
    {@const style = TEXT_STYLE[prim.role]}
    {@const attrs = {
      x: prim.x,
      y: prim.y,
      "data-role": prim.role,
      "font-size": style.size,
      "font-weight": style.weight,
      "font-style": style.italic ? "italic" : undefined,
    }}
    <!-- Span-bearing primitives are click-to-locate aids: clicking (or Enter on
         a focused one) selects their source range in the editor. The spanless
         ones (header labels) are plain, non-interactive glyphs. -->
    {#if prim.span}
      {@const span = prim.span}
      <text
        {...attrs}
        role="button"
        tabindex={0}
        class:active={isActive(span)}
        class:clickable={true}
        onclick={(e) => onPrimitiveSelect(e, span)}
        onkeydown={(e) => onPrimitiveKey(e, span)}>{prim.content}</text
      >
    {:else}
      <text {...attrs}>{prim.content}</text>
    {/if}
  {:else if prim.kind === "path"}
    {#if prim.span}
      {@const span = prim.span}
      <path
        d={prim.cmds}
        role="button"
        tabindex={0}
        fill="none"
        stroke-linecap="round"
        class:active={isActive(span)}
        class:clickable={true}
        onclick={(e) => onPrimitiveSelect(e, span)}
        onkeydown={(e) => onPrimitiveKey(e, span)}
      />
    {:else}
      <path d={prim.cmds} fill="none" stroke-linecap="round" />
    {/if}
  {/if}
{/snippet}

<svg
  class="tab"
  viewBox="0 0 {tree.meta.width} {tree.meta.height}"
  style="--tab-zoom: {zoom}"
  xmlns="http://www.w3.org/2000/svg"
  role="img"
  aria-label="tab"
>
  {#each tree.header as prim, i (i)}{@render drawPrimitive(prim)}{/each}
  {#each tree.systems as system, si (si)}
    {#each system.prims as prim, pi (pi)}{@render drawPrimitive(prim)}{/each}
    {#each system.measures as measure, mi (mi)}
      {#each measure.prims as prim, pi (pi)}{@render drawPrimitive(prim)}{/each}
    {/each}
  {/each}
</svg>

<style>
  /* Primitives reference ink/muted/accent, bound to the app's semantic theme
     tokens so the render re-themes with the rest of the UI. */
  .tab {
    --tab-ink: var(--fg);
    --tab-muted: var(--muted);
    --tab-accent: var(--accent);

    display: block;
    width: calc(100% * var(--tab-zoom));
    height: auto;
  }
  .tab line,
  .tab path {
    stroke: var(--tab-ink);
  }
  /* Paths (ties, slides, bends, choke arcs) are open curves: stroke them at a
     hairline weight and never fill, or the arc reads as a filled blob. */
  .tab path {
    fill: none;
    stroke-width: 0.07;
  }
  .tab text {
    fill: var(--tab-ink);
    text-anchor: middle;
    dominant-baseline: central;
    /* Engraved-sheet look: serif across all rendered text. */
    font-family: Georgia, "Times New Roman", serif;
  }
  /* The left-aligned header block (tuning name, string grid, capo) anchors at
     its start x rather than centring like the title and in-staff text. */
  .tab text[data-role="tuningName"],
  .tab text[data-role="tuningString"],
  .tab text[data-role="capo"] {
    text-anchor: start;
  }
  /* Hand/technique annotations read as secondary to the fret numbers; the
     header tuning block reads as secondary to the title/composer. */
  .tab text[data-role="finger"],
  .tab text[data-role="technique"],
  .tab text[data-role="strum"],
  .tab text[data-role="ending"],
  .tab text[data-role="tuningName"],
  .tab text[data-role="tuningString"],
  .tab text[data-role="capo"] {
    fill: var(--tab-muted);
  }
  /* Span-bearing primitives are interactive: clickable, and accented while the
     editor cursor sits in their source range. */
  .tab .clickable {
    cursor: pointer;
  }
  /* A mouse click focuses the primitive; don't leave the UA focus box behind.
     Keyboard navigation still gets a ring via :focus-visible. */
  .tab .clickable:focus:not(:focus-visible) {
    outline: none;
  }
  .tab text.active {
    fill: var(--tab-accent);
  }
  .tab path.active {
    stroke: var(--tab-accent);
  }
</style>
