<script lang="ts">
  import type { Snippet } from "svelte";
  import {
    type Workspace,
    type ViewInstance,
    activeTab,
    activateTab,
    toggleMaximize,
    resizePair,
    pairRatio,
    viewDef,
  } from "./workspace";
  import { splitFromPointer, clampSplit } from "./split";

  // The shell renders groups, tab strips, resize gutters, and the maximize
  // toggle; the parent supplies `view`, a snippet that mounts the right
  // component for a given tab. Layout state lives in the bound `workspace`.
  let {
    workspace = $bindable(),
    view,
  }: {
    workspace: Workspace;
    view: Snippet<[ViewInstance]>;
  } = $props();

  // When a group is maximized only it shows; otherwise the whole row, with
  // gutters between adjacent groups. Indices then line up with `workspace.groups`
  // so a gutter `i` always sits between groups `i` and `i+1`.
  const visible = $derived(
    workspace.maximizedId
      ? workspace.groups.filter((g) => g.id === workspace.maximizedId)
      : workspace.groups,
  );

  // Group DOM elements by index, so a gutter drag can measure the pair it splits
  // (correct for any number of groups, not just the N=2 case).
  let groupEls = $state<HTMLElement[]>([]);
  let dragIndex = $state(-1);

  function startDrag(i: number, e: PointerEvent) {
    dragIndex = i;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }
  function onDrag(e: PointerEvent) {
    if (dragIndex < 0) return;
    const left = groupEls[dragIndex];
    const right = groupEls[dragIndex + 1];
    if (!left || !right) return;
    const l = left.getBoundingClientRect();
    const r = right.getBoundingClientRect();
    const ratio = splitFromPointer(e.clientX, {
      left: l.left,
      width: r.right - l.left,
    });
    workspace = resizePair(workspace, dragIndex, ratio);
  }
  function endDrag(e: PointerEvent) {
    dragIndex = -1;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
  }
  function nudge(i: number, delta: number) {
    workspace = resizePair(
      workspace,
      i,
      clampSplit(pairRatio(workspace, i) + delta),
    );
  }
  function onGutterKey(i: number, e: KeyboardEvent) {
    if (e.key === "ArrowLeft") nudge(i, -0.02);
    else if (e.key === "ArrowRight") nudge(i, 0.02);
  }
</script>

<div class="workspace">
  {#each visible as g, i (g.id)}
    {@const active = activeTab(g)}
    <section class="group" bind:this={groupEls[i]} style="flex: {g.weight}">
      <div class="tabstrip">
        <div class="tabs" role="tablist">
          {#each g.tabs as tab (tab.id)}
            <button
              class="tab"
              class:active={tab.id === active?.id}
              role="tab"
              aria-selected={tab.id === active?.id}
              onclick={() => (workspace = activateTab(workspace, g.id, tab.id))}
            >
              <span class="tab-icon" aria-hidden="true"
                >{viewDef(tab.type)?.icon}</span
              >
              <span class="tab-title">{viewDef(tab.type)?.title}</span>
            </button>
          {/each}
        </div>
        <button
          class="maximize"
          aria-label={workspace.maximizedId
            ? "Restore group"
            : "Maximize group"}
          title={workspace.maximizedId ? "Restore" : "Maximize"}
          onclick={() => (workspace = toggleMaximize(workspace, g.id))}
        >
          {workspace.maximizedId ? "▢" : "▣"}
        </button>
      </div>
      <div class="group-body">
        {#if active}{@render view(active)}{/if}
      </div>
    </section>
    {#if !workspace.maximizedId && i < visible.length - 1}
      <!-- Resize divider between adjacent groups: drag, or arrow keys when
           focused. Reflects the pair's split as its slider value. -->
      <div
        class="gutter"
        class:dragging={dragIndex === i}
        role="slider"
        aria-label="Resize groups"
        aria-valuemin={15}
        aria-valuemax={85}
        aria-valuenow={Math.round(pairRatio(workspace, i) * 100)}
        tabindex="0"
        onpointerdown={(e) => startDrag(i, e)}
        onpointermove={onDrag}
        onpointerup={endDrag}
        onkeydown={(e) => onGutterKey(i, e)}
      ></div>
    {/if}
  {/each}
</div>

<style>
  .workspace {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .group {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .tabstrip {
    display: flex;
    align-items: stretch;
    justify-content: space-between;
    border-bottom: 1px solid var(--border);
    min-height: 2rem;
  }
  .tabs {
    display: flex;
    align-items: stretch;
    overflow: hidden;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    border: none;
    border-right: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    padding: 0.3rem 0.7rem;
    cursor: pointer;
    font-size: 0.8rem;
    line-height: 1;
  }
  .tab.active {
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 6%, transparent);
  }
  .tab-icon {
    font-size: 0.85rem;
  }
  .maximize {
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    padding: 0 0.6rem;
    font-size: 0.8rem;
    line-height: 1;
  }
  .maximize:hover {
    color: var(--fg);
  }
  .group-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
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
</style>
