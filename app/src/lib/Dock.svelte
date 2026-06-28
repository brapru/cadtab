<script lang="ts">
  import { projectTree, type TreeNode, type DockEntry } from "./project";
  import { tooltip } from "./tooltip";
  import Icon from "./Icon.svelte";

  // The left project dock: a collapsible panel showing the open project as a
  // folder tree — nested folders over their `.ctab` files, with open-but-unsaved
  // drafts listed as root leaves carrying a dirty dot. Toggled from the bottom
  // bar / Cmd-B (App owns `dockOpen`). Clicking a file opens (or focuses) it as
  // an editor tab; `activeKey` marks the entry the focused tab is showing.
  // Clicking a folder expands/collapses it.
  let {
    entries = [],
    projectName = "Project",
    activeKey = null,
    onOpen,
    onOpenFolder,
  }: {
    entries?: DockEntry[];
    projectName?: string;
    activeKey?: string | null;
    onOpen?: (entry: DockEntry) => void;
    onOpenFolder?: () => void;
  } = $props();

  const tree = $derived(projectTree(entries));

  // Folders are expanded by default; this holds the ones collapsed by the user,
  // keyed by folder path so the state survives tree rebuilds.
  let collapsed = $state<Record<string, true>>({});
  function toggle(path: string) {
    const { [path]: was, ...rest } = collapsed;
    collapsed = was ? rest : { ...collapsed, [path]: true };
  }
</script>

{#snippet row(node: TreeNode, depth: number)}
  {#if node.kind === "folder"}
    {@const open = !collapsed[node.path]}
    <li>
      <button
        class="row folder"
        style="--depth: {depth}"
        aria-expanded={open}
        onclick={() => toggle(node.path)}
      >
        <Icon name={open ? "folder_open" : "folder"} size={15} />
        <span class="file-name">{node.name}</span>
      </button>
      {#if open}
        <ul class="file-list nested">
          {#each node.children as child (child.kind === "folder" ? "d:" + child.path : "f:" + child.entry.key)}
            {@render row(child, depth + 1)}
          {/each}
        </ul>
      {/if}
    </li>
  {:else}
    <li>
      <button
        class="row file"
        class:active={activeKey === node.entry.key}
        class:dirty={node.entry.dirty}
        style="--depth: {depth}"
        onclick={() => onOpen?.(node.entry)}
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

<aside class="dock" aria-label="Project files">
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
  </ul>
</aside>

<style>
  .dock {
    flex: 0 0 13rem;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
    background: var(--bg);
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
  /* The unsaved dot trails the name, pushed to the row's end. */
  .dot {
    margin-left: auto;
    padding-left: 0.3rem;
    color: var(--muted);
    line-height: 1;
  }
</style>
