<script lang="ts">
  import { projectFileList, projectTree, type TreeNode } from "./project";
  import { tooltip } from "./tooltip";
  import Icon from "./Icon.svelte";

  // The left project dock: a collapsible panel showing the open project's
  // structure as a folder tree — nested folders over their `.ctab` files.
  // Toggled from the bottom bar / Cmd-B (App owns `dockOpen`). Clicking a file
  // opens (or focuses) it as an editor tab; `activePath` marks the file the
  // focused tab is showing. Clicking a folder expands/collapses it.
  let {
    entryName,
    libs = {},
    projectName = "Project",
    activePath = null,
    onOpenFile,
  }: {
    entryName: string;
    libs?: Record<string, string>;
    projectName?: string;
    activePath?: string | null;
    onOpenFile?: (path: string, isEntry: boolean) => void;
  } = $props();

  const tree = $derived(projectTree(projectFileList(entryName, libs)));

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
        use:tooltip={node.path}
        onclick={() => toggle(node.path)}
      >
        <Icon name={open ? "folder_open" : "folder"} size={15} />
        <span class="file-name">{node.name}</span>
      </button>
      {#if open}
        <ul class="file-list nested">
          {#each node.children as child (child.kind === "folder" ? "d:" + child.path : "f:" + child.file.path)}
            {@render row(child, depth + 1)}
          {/each}
        </ul>
      {/if}
    </li>
  {:else}
    <li>
      <button
        class="row file"
        class:active={activePath === node.file.path}
        style="--depth: {depth}"
        use:tooltip={node.file.path}
        onclick={() => onOpenFile?.(node.file.path, node.file.isEntry)}
      >
        <Icon name="music_note" size={15} />
        <span class="file-name">{node.name}</span>
      </button>
    </li>
  {/if}
{/snippet}

<aside class="dock" aria-label="Project files">
  <div class="dock-header">{projectName}</div>
  <ul class="file-list">
    {#each tree as node (node.kind === "folder" ? "d:" + node.path : "f:" + node.file.path)}
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
    padding: 0.45rem 0.7rem;
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
