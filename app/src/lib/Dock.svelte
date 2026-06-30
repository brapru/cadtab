<script lang="ts">
  import {
    projectTree,
    type TreeNode,
    type DockEntry,
    type DockTarget,
    type PendingEdit,
  } from "./project";
  import { tooltip } from "./tooltip";
  import Icon from "./Icon.svelte";
  import ContextMenu, { type ContextMenuItem } from "./ContextMenu.svelte";

  // The left project dock: a collapsible panel showing the open project as a
  // folder tree — nested folders over their `.ctab` files, with open-but-unsaved
  // drafts listed as root leaves carrying a dirty dot. Toggled from the bottom
  // bar / Cmd-B (App owns `dockOpen`). Clicking a file opens (or focuses) it as
  // an editor tab; `activeKey` marks the entry the focused tab is showing.
  // Clicking a folder expands/collapses it. When `canManage` (a live folder is
  // open), right-clicking offers New File/Folder, Rename, Delete; New/Rename
  // names are typed inline via a `pendingEdit` row the host drives.
  let {
    entries = [],
    dirs = [],
    projectName = "Project",
    activeKey = null,
    canManage = false,
    pendingEdit = null,
    onOpen,
    onOpenFolder,
    onContext,
    onCommitEdit,
    onCancelEdit,
  }: {
    entries?: DockEntry[];
    dirs?: string[];
    projectName?: string;
    activeKey?: string | null;
    canManage?: boolean;
    pendingEdit?: PendingEdit | null;
    onOpen?: (entry: DockEntry) => void;
    onOpenFolder?: () => void;
    onContext?: (action: string, target: DockTarget) => void;
    onCommitEdit?: (name: string) => void;
    onCancelEdit?: () => void;
  } = $props();

  const tree = $derived(projectTree(entries, dirs));

  // Folders are expanded by default; this holds the ones collapsed by the user,
  // keyed by folder path so the state survives tree rebuilds.
  let collapsed = $state<Record<string, true>>({});
  function toggle(path: string) {
    const { [path]: was, ...rest } = collapsed;
    collapsed = was ? rest : { ...collapsed, [path]: true };
  }

  // A New File/Folder targeting a collapsed folder must expand it so the inline
  // input is visible.
  $effect(() => {
    const pe = pendingEdit;
    if (
      pe &&
      pe.kind !== "rename" &&
      pe.parentPath &&
      collapsed[pe.parentPath]
    ) {
      const { [pe.parentPath]: _drop, ...rest } = collapsed;
      collapsed = rest;
    }
  });

  // The open context menu (pointer coords + the row it acted on), or null.
  let menu = $state<{ x: number; y: number; target: DockTarget } | null>(null);
  function openMenu(e: MouseEvent, target: DockTarget) {
    if (!canManage) return;
    e.preventDefault();
    e.stopPropagation();
    menu = { x: e.clientX, y: e.clientY, target };
  }
  const menuItems = $derived.by<ContextMenuItem[]>(() => {
    if (!menu) return [];
    const items: ContextMenuItem[] = [
      { label: "New File", action: "new-file" },
      { label: "New Folder", action: "new-folder" },
    ];
    if (menu.target.kind !== "root") {
      items.push({ label: "Rename", action: "rename", separatorBefore: true });
      items.push({ label: "Delete", action: "delete", destructive: true });
    }
    return items;
  });
  function selectMenu(action: string) {
    const target = menu?.target;
    menu = null;
    if (target) onContext?.(action, target);
  }

  // A file row reports `file` only when it's a real on-disk file; a draft
  // (path-null) has nothing to rename/delete, so right-clicking it acts on root.
  function fileTarget(entry: DockEntry): DockTarget {
    return entry.path !== null
      ? { kind: "file", key: entry.key, path: entry.path }
      : { kind: "root" };
  }

  // Inline-edit helpers: which row is being renamed, where a new row belongs.
  function isRenaming(node: TreeNode): boolean {
    if (!pendingEdit || pendingEdit.kind !== "rename") return false;
    return node.kind === "folder"
      ? pendingEdit.isFolder && pendingEdit.targetKey === node.path
      : !pendingEdit.isFolder && pendingEdit.targetKey === node.entry.key;
  }
  function isNewIn(path: string): boolean {
    return (
      !!pendingEdit &&
      pendingEdit.kind !== "rename" &&
      pendingEdit.parentPath === path
    );
  }
  const newIcon = $derived(
    pendingEdit?.kind === "new-folder" ? "folder" : "music_note",
  );

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
  function onEditKey(e: KeyboardEvent) {
    const input = e.currentTarget as HTMLInputElement;
    if (e.key === "Enter") {
      e.preventDefault();
      const name = input.value.trim();
      if (name && !/[\\/]/.test(name)) onCommitEdit?.(name);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onCancelEdit?.();
    }
  }
</script>

{#snippet editField(depth: number, icon: string)}
  <div class="row edit-row" style="--depth: {depth}">
    <Icon name={icon} size={15} />
    <input
      class="edit-input"
      value={pendingEdit?.initial ?? ""}
      aria-label="Name"
      use:focusSelect
      onkeydown={onEditKey}
      onblur={() => onCancelEdit?.()}
    />
  </div>
{/snippet}

{#snippet row(node: TreeNode, depth: number)}
  {#if node.kind === "folder"}
    {@const open = !collapsed[node.path]}
    <li>
      {#if isRenaming(node)}
        {@render editField(depth, open ? "folder_open" : "folder")}
      {:else}
        <button
          class="row folder"
          style="--depth: {depth}"
          aria-expanded={open}
          onclick={() => toggle(node.path)}
          oncontextmenu={(e) =>
            openMenu(e, { kind: "folder", path: node.path })}
        >
          <Icon name={open ? "folder_open" : "folder"} size={15} />
          <span class="file-name">{node.name}</span>
        </button>
      {/if}
      {#if open}
        <ul class="file-list nested" style="--depth: {depth}">
          {#each node.children as child (child.kind === "folder" ? "d:" + child.path : "f:" + child.entry.key)}
            {@render row(child, depth + 1)}
          {/each}
          {#if isNewIn(node.path)}
            <li>{@render editField(depth + 1, newIcon)}</li>
          {/if}
        </ul>
      {/if}
    </li>
  {:else if isRenaming(node)}
    <li>{@render editField(depth, "music_note")}</li>
  {:else}
    <li>
      <button
        class="row file"
        class:active={activeKey === node.entry.key}
        class:dirty={node.entry.dirty}
        style="--depth: {depth}"
        onclick={() => onOpen?.(node.entry)}
        oncontextmenu={(e) => openMenu(e, fileTarget(node.entry))}
      >
        <Icon name="music_note" size={15} />
        <span class="file-name">{node.name}</span>
        {#if node.entry.dirty}
          <span class="dot" aria-label="unsaved">•</span>
        {/if}
      </button>
    </li>
  {/if}
{/snippet}

<aside
  class="dock"
  aria-label="Project files"
  oncontextmenu={(e) => openMenu(e, { kind: "root" })}
>
  <div class="dock-header">
    <span class="dock-title">{projectName}</span>
    {#if onOpenFolder}
      <button
        class="dock-action"
        aria-label="Open Folder"
        use:tooltip={"Open folder (Cmd/Ctrl+Shift+O)"}
        onclick={() => onOpenFolder?.()}
      >
        <Icon name="folder_open" size={16} />
      </button>
    {/if}
  </div>
  <ul class="file-list">
    {#each tree as node (node.kind === "folder" ? "d:" + node.path : "f:" + node.entry.key)}
      {@render row(node, 0)}
    {/each}
    {#if isNewIn("")}
      <li>{@render editField(0, newIcon)}</li>
    {/if}
  </ul>
  {#if menu}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      items={menuItems}
      onSelect={selectMenu}
      onDismiss={() => (menu = null)}
    />
  {/if}
</aside>

<style>
  .dock {
    flex: 0 0 13rem;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
    background: var(--bg-panel);
    overflow: hidden;
  }
  .dock-header {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.4rem 0.3rem 0.7rem;
    border-bottom: 1px solid var(--border);
  }
  .dock-title {
    flex: 1;
    min-width: 0;
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* The dock-header Open Folder control (desktop): a quiet square icon button. */
  .dock-action {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 1.5rem;
    height: 1.5rem;
    border: none;
    background: transparent;
    color: var(--muted);
    border-radius: 0.25rem;
    cursor: pointer;
    padding: 0;
  }
  .dock-action:hover {
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 8%, transparent);
  }
  .file-list {
    list-style: none;
    margin: 0;
    padding: 0.25rem 0;
    overflow: auto;
    min-height: 0;
  }
  .file-list.nested {
    padding: 0;
    overflow: visible;
    position: relative;
  }
  /* Vertical indent guide down the left of a folder's children — spans the full
     height of the nested block (top→bottom of its contents) and sits under the
     parent folder's icon, offset via the folder's --depth. */
  .file-list.nested::before {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: calc(0.7rem + var(--depth) * 0.85rem + 0.45rem);
    width: 1px;
    background: var(--border);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    width: 100%;
    /* indent grows with tree depth; the base pad keeps the first level off the edge */
    padding: 0.2rem 0.7rem 0.2rem calc(0.7rem + var(--depth) * 0.85rem);
    border: none;
    background: transparent;
    font: inherit;
    font-size: 0.82rem;
    text-align: left;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    cursor: pointer;
  }
  .row :global(.material-symbols-outlined) {
    flex: 0 0 auto;
    opacity: 0.75;
  }
  .row:hover {
    color: var(--fg);
  }
  .file.active {
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 6%, transparent);
  }
  .file.dirty {
    color: var(--fg);
  }
  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* An inline name-edit row reuses the row layout; its input fills the rest. */
  .edit-row {
    cursor: default;
  }
  .edit-input {
    flex: 1;
    min-width: 0;
    border: 1px solid var(--accent);
    border-radius: 0.2rem;
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    font-size: 0.82rem;
    padding: 0 0.2rem;
    outline: none;
  }
  /* The unsaved dot trails the name, pushed to the row's end. */
  .dot {
    margin-left: auto;
    padding-left: 0.3rem;
    color: var(--muted);
    line-height: 1;
  }
</style>
