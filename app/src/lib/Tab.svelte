<script lang="ts">
  import type { RenderTree, Primitive } from "./types";

  let { tree }: { tree: RenderTree } = $props();
</script>

{#snippet drawPrimitive(prim: Primitive)}
  {#if prim.kind === "line"}
    <line
      x1={prim.x1}
      y1={prim.y1}
      x2={prim.x2}
      y2={prim.y2}
      stroke-width={prim.weight}
    />
  {:else if prim.kind === "text"}
    <text x={prim.x} y={prim.y} data-role={prim.role}>{prim.content}</text>
  {:else if prim.kind === "path"}
    <path d={prim.cmds} />
  {/if}
{/snippet}

<svg
  class="tab"
  viewBox="0 0 {tree.meta.width} {tree.meta.height}"
  xmlns="http://www.w3.org/2000/svg"
  role="img"
  aria-label="tab"
>
  {#each tree.header as prim, i (i)}{@render drawPrimitive(prim)}{/each}
  {#each tree.systems as system, si (si)}
    {#each system.measures as measure, mi (mi)}
      {#each measure.prims as prim, pi (pi)}{@render drawPrimitive(prim)}{/each}
    {/each}
  {/each}
</svg>

<style>
  .tab {
    display: block;
    width: 100%;
    height: auto;
  }
  .tab line {
    stroke: currentColor;
  }
  .tab text {
    fill: currentColor;
    font-size: 1.4px;
    text-anchor: middle;
    dominant-baseline: central;
  }
</style>
