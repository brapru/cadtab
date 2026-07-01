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
  import Icon from "./Icon.svelte";
  import { tooltip } from "./tooltip";

  // The shell renders groups, tab strips, resize gutters, and the maximize
  // toggle; the parent supplies `view`, a snippet that mounts the right
  // component for a given tab. Layout state lives in the bound `workspace`.
  let {
    workspace = $bindable(),
    view,
    missingDocIds = [],
    docName,
    docDirty,
    previewable,
    onActivateView,
    onCloseTab,
    onOpenRender,
    onOpenPreview,
    onNew,
    newTemplates = [],
    onFit,
  }: {
    workspace: Workspace;
    view: Snippet<[ViewInstance]>;
    // Doc ids whose backing file was deleted/moved on disk; their tabs render
    // struck-through until the doc is saved back.
    missingDocIds?: readonly string[];
    // Resolve a doc id to its display filename (D49: tabs label by filename, the
    // icon carries the view-type distinction). Falls back to the view's registry
    // title when unset (singletons, or a doc with no name yet).
    docName?: (docId: string) => string | null | undefined;
    // Whether a doc has unsaved changes — drives the tab's edited-dot, shown
    // just left of the filename (every view of a dirty doc carries it).
    docDirty?: (docId: string) => boolean;
    // Whether a doc has laid-out content worth previewing (compiles to systems —
    // scores and def-libraries alike, but not empty/error docs). Gates the
    // preview launcher (T7.43).
    previewable?: (docId: string) => boolean;
    onActivateView?: (instance: ViewInstance) => void;
    onCloseTab?: (instance: ViewInstance) => void;
    onOpenRender?: (docId: string) => void;
    onOpenPreview?: (docId: string) => void;
    onNew?: (templateId: string) => void;
    newTemplates?: readonly { id: string; label: string }[];
    onFit?: () => void;
  } = $props();

  // The New ("+") control's open template menu, keyed by the control that owns it
  // (a group id, or "empty" for the no-tabs placeholder) so only one is open.
  let newMenuKey = $state<string | null>(null);
  function toggleNewMenu(key: string) {
    newMenuKey = newMenuKey === key ? null : key;
  }
  function chooseNew(id: string) {
    newMenuKey = null;
    onNew?.(id);
  }
  // Dismiss the menu on Escape or a pointer down outside it.
  $effect(() => {
    if (newMenuKey === null) return;
    function onPointer(e: PointerEvent) {
      const t = e.target;
      if (t instanceof Element && t.closest(".new-wrap")) return;
      newMenuKey = null;
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") newMenuKey = null;
    }
    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("keydown", onKey);
    };
  });

  // The group whose control set (New / Fit / split / maximize) is shown — the one
  // last interacted with (a pointer down anywhere inside it), defaulting to the
  // first. A maximized group is the only one visible, so it owns the controls.
  // Tracked locally rather than derived from the active doc, since the default
  // editor|render layout puts one doc's views in two groups — doc id alone can't
  // tell which group is focused.
  let activeGroupId = $state<string | null>(null);
  const controlGroupId = $derived(
    workspace.maximizedId ??
      (activeGroupId && workspace.groups.some((g) => g.id === activeGroupId)
        ? activeGroupId
        : (workspace.groups[0]?.id ?? null)),
  );

  // Whether a document already has an open view of `type` anywhere in the layout
  // — drives the editor tab's render launcher (spawn vs. jump-to).
  function hasView(type: string, docId: string | null): boolean {
    return (
      docId !== null &&
      workspace.groups.some((g) =>
        g.tabs.some((t) => t.type === type && t.docId === docId),
      )
    );
  }

  // A tab's visible label: the document's filename when known, else the view's
  // registry title (singletons, or before a doc has a name). The icon — not the
  // text — distinguishes editor/render/preview, so every view of one file shares
  // the filename and the missing-on-disk strike rides that filename.
  function tabLabel(tab: ViewInstance): string {
    const name = tab.docId !== null ? docName?.(tab.docId) : null;
    return name ?? viewDef(tab.type)?.title ?? "";
  }

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

  // Tab drag between groups, built on pointer
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

<!-- The New ("+") control: a button that opens a template menu, reused by
     each group's controls and the empty-tabs placeholder. `key` scopes which
     menu is open. -->
{#snippet newControl(key: string)}
  <div class="new-wrap">
    <button
      class="new"
      aria-label="New tab"
      aria-haspopup="menu"
      aria-expanded={newMenuKey === key}
      use:tooltip={"New tab"}
      onclick={() => toggleNewMenu(key)}
    >
      <Icon name="add" size={16} />
    </button>
    {#if newMenuKey === key}
      <div class="new-menu" role="menu">
        {#each newTemplates as t (t.id)}
          <button
            class="new-item"
            role="menuitem"
            onclick={() => chooseNew(t.id)}
          >
            {t.label}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

<div class="workspace">
  {#each visible as g, i (g.id)}
    {@const active = activeTab(g)}
    <section
      class="group"
      bind:this={groupEls[i]}
      style="flex: {flexGrow(g.weight)}"
      role="group"
      onpointerdowncapture={() => (activeGroupId = g.id)}
    >
      <div class="tabstrip">
        <div class="tabs" role="tablist">
          {#each g.tabs as tab (tab.id)}
            <div class="tab-wrap" class:active={tab.id === active?.id}>
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
                ondblclick={() => (workspace = toggleMaximize(workspace, g.id))}
              >
                <span class="tab-icon">
                  <Icon name={viewDef(tab.type)?.icon ?? ""} size={16} />
                </span>
                {#if tab.docId !== null && docDirty?.(tab.docId)}
                  <span class="tab-dot" aria-label="unsaved">•</span>
                {/if}
                <span
                  class="tab-title"
                  class:missing={tab.docId !== null &&
                    missingDocIds.includes(tab.docId)}
                >
                  {tabLabel(tab)}
                </span>
              </button>
              <button
                class="tab-close"
                aria-label="Close tab"
                use:tooltip={"Close tab"}
                onclick={() => onCloseTab?.(tab)}
              >
                <Icon name="close" size={14} />
              </button>
            </div>
          {/each}
        </div>
        <!-- The open space after the tabs, where a dragged tab lands; it grows to
             fill the strip and is the only region the drop cue highlights. -->
        <div
          class="dropzone"
          class:droptarget={dragOverId === g.id && draggingId !== null}
        ></div>
        <!-- The control set lives on the active (last-interacted) group only; the
             per-tab close/launcher stay on every tab. -->
        <div class="group-actions">
          {#if g.id === controlGroupId}
            {@render newControl(g.id)}
            {#if active?.type === "editor"}
              {@const open = hasView("render", active.docId)}
              <!-- Open/jump to the active editor's render: closes the gap
                   where a closed render had no way back. -->
              <button
                class="launch"
                class:open
                aria-label={open ? "Go to render" : "Open render"}
                use:tooltip={open ? "Go to render" : "Open render"}
                onclick={() => active?.docId && onOpenRender?.(active.docId)}
              >
                <Icon name="music_note" size={16} fill={open} />
              </button>
              {#if active.docId !== null && previewable?.(active.docId)}
                {@const previewing = hasView("preview", active.docId)}
                <!-- Print preview launcher, offered only for score docs (T7.43);
                     moved here from the topbar. Open/jump-to like the render. -->
                <button
                  class="launch"
                  class:open={previewing}
                  aria-label={previewing ? "Go to preview" : "Open preview"}
                  use:tooltip={previewing
                    ? "Go to print preview"
                    : "Open print preview (final light output)"}
                  onclick={() => active?.docId && onOpenPreview?.(active.docId)}
                >
                  <Icon name="preview" size={16} fill={previewing} />
                </button>
              {/if}
            {/if}
            {#if active?.type === "render"}
              <!-- Fit resets zoom to fill the pane width. Shown when this group
                   is showing a render. -->
              <button
                class="fit"
                aria-label="Fit to width"
                use:tooltip={{ title: "Fit to width", shortcut: "mod 0" }}
                onclick={() => onFit?.()}
              >
                <Icon name="crop_free" size={16} />
              </button>
            {/if}
            {#if g.tabs.length > 1}
              <button
                class="split"
                aria-label="Split group"
                use:tooltip={{
                  title: "Split",
                  description: "Split the active tab into its own group",
                }}
                onclick={() => (workspace = splitTab(workspace, g.id))}
              >
                <Icon name="split_scene" size={16} />
              </button>
            {/if}
            <button
              class="maximize"
              aria-label={workspace.maximizedId
                ? "Restore group"
                : "Maximize group"}
              use:tooltip={workspace.maximizedId ? "Restore" : "Maximize"}
              onclick={() => (workspace = toggleMaximize(workspace, g.id))}
            >
              <Icon
                name={workspace.maximizedId
                  ? "close_fullscreen"
                  : "open_in_full"}
                size={16}
              />
            </button>
          {/if}
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
  {#if visible.length === 0}
    <!-- No open tabs: keep a New control reachable here, so an emptied
         workspace can still spawn a document. -->
    <div class="empty">
      <p>No open tabs</p>
      {@render newControl("empty")}
    </div>
  {/if}
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
    border-bottom: 1px solid var(--border);
    min-height: 2rem;
    /* The tab strip sits at panel level with the dock — the active tab drops to
       the editor surface and inactive tabs stay at this panel tone (T7.34b). */
    background: var(--bg-panel);
  }
  /* The empty strip space after the tabs (before the controls); it grows to push
     the controls to the right. */
  .dropzone {
    flex: 1;
  }
  /* Drag cue: while a tab is dragged, highlight only this open space —
     where the tab would land — not the existing tabs, the view body, or the whole
     group. A translucent accent wash plus an accent bottom edge. */
  .dropzone.droptarget {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    box-shadow: inset 0 -1px 0 var(--accent);
  }
  .tabs {
    display: flex;
    align-items: stretch;
    overflow: hidden;
  }
  /* A tab is a label button plus a close button, sharing one bordered cell so
     the active tint covers both. The label keeps the drag/click handlers. */
  .tab-wrap {
    display: flex;
    align-items: stretch;
    border-right: 1px solid var(--border);
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    border: none;
    background: transparent;
    color: var(--muted);
    padding: 0.3rem 0.3rem 0.3rem 0.7rem;
    cursor: pointer;
    font-size: 0.8rem;
    line-height: 1;
    /* Pointer-driven drag: keep touch gestures from scrolling mid-drag. */
    touch-action: none;
  }
  /* The active tab drops to the editor surface so it reads as cut out of the
     panel-tone strip and continuous with the editor below (Zed-style) — no top
     accent bar. Inactive tabs stay flush with the strip (transparent panel) and
     keep muted label text; the active tab gets full-strength ink. */
  .tab-wrap.active {
    background: var(--bg-editor);
  }
  .tab-wrap.active .tab {
    color: var(--fg);
  }
  .tab.dragging {
    opacity: 0.5;
  }
  .tab-icon {
    display: flex;
    align-items: center;
  }
  /* The unsaved-changes dot sits just left of the filename (T7.39). The negative
     right margin pulls it tight to the name so the pair reads as one unit rather
     than floating midway between the icon and the title; it follows the tab's
     text colour (muted inactive, --fg active). */
  .tab-dot {
    margin-right: -0.18rem;
    line-height: 1;
    user-select: none;
  }
  /* A tab whose backing file was deleted/moved on disk: struck through, dimmed —
     the buffer is still editable and a Save rewrites the file. */
  .tab-title.missing {
    text-decoration: line-through;
    opacity: 0.6;
  }
  .tab-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.6rem;
    border: none;
    background: transparent;
    color: var(--muted);
    padding: 0;
    cursor: pointer;
    opacity: 0.6;
  }
  .tab-close:hover {
    opacity: 1;
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 12%, transparent);
  }
  .group-actions {
    display: flex;
    align-items: center;
    padding: 0 0.2rem;
  }
  /* Icon-only chrome buttons are uniform squares (explicit equal width/height,
     not stretch) sitting flush next to each other, each with its own square
     hover highlight. */
  .new,
  .launch,
  .fit,
  .split,
  .maximize {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.9rem;
    height: 1.9rem;
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
    border-radius: 0.3rem;
    font-size: 0.8rem;
    line-height: 1;
  }
  .new:hover,
  .launch:hover,
  .fit:hover,
  .split:hover,
  .maximize:hover {
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 12%, transparent);
  }
  /* When the active editor's render is already open, the launcher reads as a
     jump-to toggle. */
  .launch.open {
    color: var(--accent);
  }
  /* The New control anchors its template menu just below the "+". */
  .new-wrap {
    position: relative;
    display: flex;
    align-items: stretch;
  }
  .new-menu {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 10;
    display: flex;
    flex-direction: column;
    min-width: 9rem;
    padding: 0.25rem;
    background: var(--bg-chrome);
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    box-shadow: var(--shadow-popup);
  }
  .new-item {
    border: none;
    background: transparent;
    color: var(--fg);
    text-align: left;
    padding: 0.35rem 0.55rem;
    border-radius: 0.25rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.82rem;
    white-space: nowrap;
  }
  .new-item:hover {
    background: color-mix(in srgb, var(--fg) 10%, transparent);
  }
  /* The no-tabs placeholder, centred in the empty workspace. */
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    color: var(--muted);
  }
  .empty p {
    margin: 0;
    font-size: 0.9rem;
  }
  .empty .new {
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    padding: 0.3rem 0.7rem;
  }
  .empty .new-menu {
    right: auto;
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
