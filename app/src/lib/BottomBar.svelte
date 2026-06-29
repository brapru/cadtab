<script lang="ts">
  import type { Diagnostic } from "./types";
  import { diagnosticCounts } from "./diagnostics";
  import { tooltip } from "./tooltip";

  // The bottom status bar: a small, unobtrusive strip
  // hosting the dock toggle and a live problem indicator. It sets the
  // bottom-control styling the dock, diagnostics panel, and theme
  // switcher slot into.
  let {
    diagnostics = [],
    dockOpen = false,
    notice = null,
    onToggleDock,
  }: {
    diagnostics?: Diagnostic[];
    dockOpen?: boolean;
    // A transient status flash (e.g. "Exported tune.pdf"); when set it takes the
    // diagnostics slot for a few seconds, then the caller clears it.
    notice?: string | null;
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
      use:tooltip={"Toggle project dock (Cmd/Ctrl+B)"}
      onclick={() => onToggleDock?.()}
    >
      <span aria-hidden="true">◧</span>
    </button>
  </div>
  <div class="group">
    {#if notice}
      <!-- A transient export/success flash, taking the diagnostics slot. -->
      <div class="notice" role="status">
        <span class="ok" aria-hidden="true">✓</span>
        <span class="text">{notice}</span>
      </div>
    {:else}
      <!-- Live problem counts. -->
      <div
        class="diagnostics"
        class:clean
        use:tooltip={clean
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
    {/if}
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
  /* The export-success flash: the check reads as confirmation, the text in
     full-strength ink, fading in so the change registers. */
  .notice {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0 0.4rem;
    color: var(--fg);
    animation: notice-in 0.18s ease-out;
  }
  .notice .ok {
    color: var(--ok, #3fa45b);
    font-weight: 700;
  }
  @keyframes notice-in {
    from {
      opacity: 0;
      transform: translateY(2px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .notice {
      animation: none;
    }
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
