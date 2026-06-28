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
    moveTab,
    splitTab,
    viewDef,
  } from "./workspace";
  import { splitFromPointer, clampSplit } from "./split";

  // The shell renders groups, tab strips, resize gutters, and the maximize
  // toggle; the parent supplies `view`, a snippet that mounts the right
  // component for a given tab. Layout state lives in the bound `workspace`.
  let {
    workspace = $bindable(),
    view,
    onActivateView,
  }: {
    workspace: Workspace;
    view: Snippet<[ViewInstance]>;
    onActivateView?: (instance: ViewInstance) => void;
  } = $props();

  function activate(groupId: string, tab: ViewInstance) {
    workspace = activateTab(workspace, groupId, tab.id);
    onActivateView?.(tab);
  }

  // When a group is maximized only it shows; otherwise the whole row, with
  // gutters between adjacent groups. Indices then line up with `workspace.groups`
  // so a gutter `i` always sits between groups `i` and `i+1`.
  const visible = $derived(
    workspace.maximizedId
      ? workspace.groups.filter((g) => g.id === workspace.maximizedId)
      : workspace.groups,
  );

  // Normalize each group's flex-grow over the visible set so the row always
  // fills. Raw weights can sum to under 1 after move→split→move churn (or when a
  // sub-1 group is maximized alone), and a `flex-grow` total below 1 leaves the
  // rest of the row empty — cutting the view off. Dividing by the total keeps the
  // groups' relative proportions while making the grows sum to 1.
  const totalWeight = $derived(visible.reduce((sum, g) => sum + g.weight, 0));
  function flexGrow(weight: number): number {
    return totalWeight > 0 ? weight / totalWeight : 1;
  }

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

  // Tab drag between groups (D41 "move a tab between groups"), built on pointer
  // events — not HTML5 drag-and-drop — so it works in WKWebView (the desktop
  // webview), where in-page HTML5 DnD is intercepted/unreliable. The gutter uses
  // the same approach; the split button is the keyboard-reachable counterpart.
  const DRAG_THRESHOLD = 5;
  let pressId: string | null = null;
  let pressX = 0;
  let didDrag = false;
  let draggingId = $state<string | null>(null);
  let dragOverId = $state<string | null>(null);

  // Pointer capture keeps move/up coming to the tab even over other groups;
  // guarded since it can throw if the pointer isn't actively down.
  function capture(e: PointerEvent, on: boolean) {
    const el = e.currentTarget as HTMLElement;
    try {
      if (on) el.setPointerCapture?.(e.pointerId);
      else el.releasePointerCapture?.(e.pointerId);
    } catch {
      /* no active pointer — ignore */
    }
  }

  function onTabPointerDown(id: string, e: PointerEvent) {
    if (e.button !== 0) return;
    pressId = id;
    pressX = e.clientX;
    didDrag = false;
    capture(e, true);
  }
  function onTabPointerMove(e: PointerEvent) {
    if (pressId === null) return;
    if (draggingId === null) {
      if (Math.abs(e.clientX - pressX) < DRAG_THRESHOLD) return;
      draggingId = pressId; // crossed the threshold — this is a drag, not a click
      didDrag = true;
    }
    dragOverId = groupAtX(e.clientX);
  }
  function onTabPointerUp(e: PointerEvent) {
    capture(e, false);
    if (draggingId && dragOverId) {
      workspace = moveTab(workspace, draggingId, dragOverId);
    }
    pressId = null;
    draggingId = null;
    dragOverId = null;
  }
  // The group whose horizontal extent contains x (groups span the full height).
  function groupAtX(x: number): string | null {
    for (let i = 0; i < visible.length; i++) {
      const r = groupEls[i]?.getBoundingClientRect();
      if (r && x >= r.left && x <= r.right) return visible[i].id;
    }
    return null;
  }
  // A tab press activates it on click, unless the press turned into a drag.
  function onTabClick(groupId: string, tab: ViewInstance) {
    if (didDrag) {
      didDrag = false;
      return;
    }
    activate(groupId, tab);
  }
</script>

<div class="workspace">
  {#each visible as g, i (g.id)}
    {@const active = activeTab(g)}
    <section
      class="group"
      class:droptarget={dragOverId === g.id && draggingId !== null}
      bind:this={groupEls[i]}
      style="flex: {flexGrow(g.weight)}"
      role="group"
    >
      <div class="tabstrip">
        <div class="tabs" role="tablist">
          {#each g.tabs as tab (tab.id)}
            <button
              class="tab"
              class:active={tab.id === active?.id}
              class:dragging={draggingId === tab.id}
              role="tab"
              aria-selected={tab.id === active?.id}
              onpointerdown={(e) => onTabPointerDown(tab.id, e)}
              onpointermove={onTabPointerMove}
              onpointerup={onTabPointerUp}
              onclick={() => onTabClick(g.id, tab)}
            >
              <span class="tab-icon" aria-hidden="true"
                >{viewDef(tab.type)?.icon}</span
              >
              <span class="tab-title">{viewDef(tab.type)?.title}</span>
            </button>
          {/each}
        </div>
        <div class="group-actions">
          {#if g.tabs.length > 1}
            <button
              class="split"
              aria-label="Split group"
              title="Split the active tab into its own group"
              onclick={() => (workspace = splitTab(workspace, g.id))}
            >
              ◫
            </button>
          {/if}
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
  /* Cue the group a dragged tab would drop into. */
  .group.droptarget {
    outline: 2px solid color-mix(in srgb, var(--accent) 60%, transparent);
    outline-offset: -2px;
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
    /* Pointer-driven drag: keep touch gestures from scrolling mid-drag. */
    touch-action: none;
  }
  .tab.active {
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 6%, transparent);
  }
  .tab.dragging {
    opacity: 0.5;
  }
  .tab-icon {
    font-size: 0.85rem;
  }
  .group-actions {
    display: flex;
    align-items: stretch;
  }
  .split,
  .maximize {
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    padding: 0 0.6rem;
    font-size: 0.8rem;
    line-height: 1;
  }
  .split:hover,
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
