<script lang="ts">
  import type { RenderTree, Primitive, TextRole } from "./types";

  // The painter is thin: it positions primitives verbatim in the layout's
  // logical coordinate space (1 unit = string spacing) and lets the SVG viewBox
  // scale them to pixels. `zoom` multiplies the fit-to-container width.
  let { tree, zoom = 1 }: { tree: RenderTree; zoom?: number } = $props();

  // Per-role text metrics, in logical units, tuned to the row heights the layout
  // engine reserves for header rows so glyphs sit inside their allotted space.
  // Intent lives in the role; geometry stays in the coordinates.
  type TextStyle = { size: number; weight?: number; italic?: boolean };
  const TEXT_STYLE: Record<TextRole, TextStyle> = {
    title: { size: 1.5, weight: 600 },
    composer: { size: 0.9 },
    tempo: { size: 0.85 },
    tuning: { size: 0.85 },
    capo: { size: 0.8 },
    fretNumber: { size: 1.3 },
    stringLabel: { size: 1.1 },
    finger: { size: 0.95 },
    strum: { size: 1.5 },
    technique: { size: 0.95, italic: true },
    ending: { size: 0.95 },
    rest: { size: 1.5 },
  };
</script>

{#snippet drawPrimitive(prim: Primitive)}
  {#if prim.kind === "line"}
    <line
      x1={prim.x1}
      y1={prim.y1}
      x2={prim.x2}
      y2={prim.y2}
      stroke-width={prim.weight}
      stroke-linecap="round"
    />
  {:else if prim.kind === "text"}
    {@const style = TEXT_STYLE[prim.role]}
    <text
      x={prim.x}
      y={prim.y}
      data-role={prim.role}
      font-size={style.size}
      font-weight={style.weight}
      font-style={style.italic ? "italic" : undefined}>{prim.content}</text
    >
  {:else if prim.kind === "path"}
    <path d={prim.cmds} fill="none" stroke-linecap="round" />
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
  /* Theming tokens: a single ink/muted/accent palette the primitives reference,
     re-themed under dark mode. A future toggle just rebinds these. */
  .tab {
    --tab-ink: #1a1a1a;
    --tab-muted: #5f5f5f;
    --tab-accent: #b4540a;

    display: block;
    width: calc(100% * var(--tab-zoom));
    height: auto;
  }
  @media (prefers-color-scheme: dark) {
    .tab {
      --tab-ink: #e6e6e6;
      --tab-muted: #9a9a9a;
      --tab-accent: #e08a3c;
    }
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
  }
  /* Hand/technique annotations read as secondary to the fret numbers. */
  .tab text[data-role="finger"],
  .tab text[data-role="technique"],
  .tab text[data-role="strum"],
  .tab text[data-role="ending"] {
    fill: var(--tab-muted);
  }
</style>
