<script lang="ts">
  import type { Diagnostic } from "./types";
  import { diagnosticCounts } from "./diagnostics";

  // The bottom status bar (D41 global singleton): a small, unobtrusive strip
  // hosting the dock toggle and a live problem indicator. It sets the
  // bottom-control styling the dock (T7.2), diagnostics panel (T4.7m), and theme
  // switcher (T7.12) slot into.
  let {
    diagnostics = [],
    dockOpen = false,
    onToggleDock,
  }: {
    diagnostics?: Diagnostic[];
    dockOpen?: boolean;
    onToggleDock?: () => void;
  } = $props();

  const counts = $derived(diagnosticCounts(diagnostics));
  const clean = $derived(counts.errors === 0 && counts.warnings === 0);
</script>

<footer class="bottombar">
  <div class="group">
    <button
      class="control dock-toggle"
      class:active={dockOpen}
      aria-label="Toggle project dock"
      aria-pressed={dockOpen}
      title="Toggle project dock (Cmd/Ctrl+B)"
      onclick={() => onToggleDock?.()}
    >
      <span aria-hidden="true">◧</span>
    </button>
  </div>
  <div class="group">
    <!-- Live problem counts. T4.7m turns this into a button opening an
         exhaustive diagnostics panel that jumps the editor to a clicked entry. -->
    <div
      class="diagnostics"
      class:clean
      title={clean
        ? "No problems"
        : `${counts.errors} error(s), ${counts.warnings} warning(s)`}
    >
      {#if clean}
        <span class="ok" aria-hidden="true">✓</span>
        <span class="text">No problems</span>
      {:else}
        <span class="count error">
          <span class="dot" aria-hidden="true">●</span>
          <span class="num">{counts.errors}</span>
        </span>
        <span class="count warning">
          <span class="dot" aria-hidden="true">▲</span>
          <span class="num">{counts.warnings}</span>
        </span>
      {/if}
    </div>
  </div>
</footer>

<style>
  .bottombar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 1.7rem;
    padding: 0 0.4rem;
    border-top: 1px solid var(--border);
    background: var(--bg);
    color: var(--muted);
    font-size: 0.72rem;
    line-height: 1;
    user-select: none;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 0.2rem;
  }
  /* The shared bottom-control look: borderless, compact, lights up on hover. */
  .control {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    height: 1.7rem;
    padding: 0 0.4rem;
    font: inherit;
  }
  .control:hover,
  .control.active {
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 8%, transparent);
  }
  .diagnostics {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0 0.4rem;
  }
  .diagnostics.clean .ok {
    color: var(--muted);
  }
  .count {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    font-variant-numeric: tabular-nums;
  }
  .count .dot {
    font-size: 0.6rem;
  }
  .count.error .dot {
    color: var(--error);
  }
  .count.warning .dot {
    color: var(--warning);
  }
</style>
