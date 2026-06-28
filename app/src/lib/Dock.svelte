<script lang="ts">
  import { projectFileList } from "./project";

  // The left project dock (D41 global singleton): a collapsible panel showing the
  // open project's structure — the entry document plus its importable libs (D38).
  // Toggled from the bottom bar / Cmd-B (App owns `dockOpen`). Display-only for
  // now; opening a file as an editor tab arrives with multi-file editing (T7.4).
  let {
    entryName,
    libs = {},
    projectName = "Project",
  }: {
    entryName: string;
    libs?: Record<string, string>;
    projectName?: string;
  } = $props();

  const files = $derived(projectFileList(entryName, libs));
</script>

<aside class="dock" aria-label="Project files">
  <div class="dock-header">{projectName}</div>
  <ul class="file-list">
    {#each files as f (f.path)}
      <li class="file" class:active={f.isEntry} title={f.path}>
        <span class="file-icon" aria-hidden="true">♪</span>
        <span class="file-name">{f.name}</span>
      </li>
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
  .file {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.2rem 0.7rem;
    font-size: 0.82rem;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
  }
  .file.active {
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 6%, transparent);
  }
  .file-icon {
    font-size: 0.75rem;
    opacity: 0.7;
  }
  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
