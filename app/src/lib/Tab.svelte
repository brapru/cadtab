<script lang="ts">
  import type { RenderTree, Primitive, Span, TextRole } from "./types";
  import { spansOverlap } from "./mapping";
  import { TEXT_STYLE, textAnchor, isMuted } from "./tabStyle";

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

  // The role → CSS class for anchor + muting, both derived from tabStyle.ts's
  // shared `textAnchor`/`isMuted` — the same source svg.ts/export read (T7.37),
  // so the screen render can never drift from the exported artifact.
  function roleClass(role: TextRole) {
    const anchor = textAnchor(role);
    return [
      anchor === "start"
        ? "anchor-start"
        : anchor === "end"
          ? "anchor-end"
          : "",
      isMuted(role) ? "muted" : "",
    ]
      .filter(Boolean)
      .join(" ");
  }

  // Geometry of the soft selection chip behind an active glyph (T7.32). The
  // painter carries no font metrics, so the width is estimated from the glyph
  // count and size; the generous padding absorbs that approximation. The chip
  // grows away from the text anchor to match how the glyph is laid out.
  function textChip(prim: Extract<Primitive, { kind: "text" }>) {
    const { size } = TEXT_STYLE[prim.role];
    const w = prim.content.length * size * 0.6 + 0.6;
    const h = size * 0.95 + 0.48;
    const anchor = textAnchor(prim.role);
    const x =
      anchor === "start"
        ? prim.x - 0.3
        : anchor === "end"
          ? prim.x - w + 0.3
          : prim.x - w / 2;
    return { x, y: prim.y - h / 2, w, h, r: 0.36 };
  }

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
      {@const active = isActive(span)}
      {#if active}
        {@const chip = textChip(prim)}
        <!-- Soft selection chip painted behind the active glyph (drawn first so
             it sits under the ink); pointer-events off so the number stays the
             click target. -->
        <rect
          class="active-chip"
          x={chip.x}
          y={chip.y}
          width={chip.w}
          height={chip.h}
          rx={chip.r}
        />
      {/if}
      <text
        {...attrs}
        class="{roleClass(prim.role)} clickable"
        class:active
        role="button"
        tabindex={0}
        onclick={(e) => onPrimitiveSelect(e, span)}
        onkeydown={(e) => onPrimitiveKey(e, span)}>{prim.content}</text
      >
    {:else}
      <text {...attrs} class={roleClass(prim.role)}>{prim.content}</text>
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
    --tab-select: var(--select);

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
    /* Engraved-sheet look: self-hosted Source Serif 4 (app.css), the same face
       embedded into PDF exports so screen and print match; serif fallback. */
    font-family: "Source Serif 4", Georgia, serif;
  }
  /* Anchor + muting are role-driven, but the role→class mapping comes from
     tabStyle.ts's shared `textAnchor`/`isMuted` (roleClass()), the same source
     svg.ts/export read — so screen and export can't diverge (T7.37). */
  .tab text.anchor-start {
    text-anchor: start;
  }
  .tab text.anchor-end {
    text-anchor: end;
  }
  .tab text.muted {
    fill: var(--tab-muted);
  }
  /* Span-bearing primitives are interactive: clickable, and highlighted while
     the editor cursor sits in their source range. */
  .tab .clickable {
    cursor: pointer;
  }
  /* A mouse click focuses the primitive; don't leave the UA focus box behind.
     Keyboard navigation still gets a ring via :focus-visible. */
  .tab .clickable:focus:not(:focus-visible) {
    outline: none;
  }
  /* The cursor<->render active primitive reads as a calm selection, not a
     recolour (T7.32): the glyph keeps its ink and sits on a soft --select chip,
     like highlighted text. The chip is decorative — clicks pass through to the
     number. Open-curve paths (ties/slides/bends) have no interior to fill, so
     they take the calm --select stroke instead. */
  .tab .active-chip {
    fill: color-mix(in srgb, var(--tab-select) 42%, transparent);
    pointer-events: none;
  }
  .tab path.active {
    stroke: var(--tab-select);
    stroke-width: 0.1;
  }
</style>
