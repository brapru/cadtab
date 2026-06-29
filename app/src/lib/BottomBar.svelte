<script lang="ts">
  import type { Diagnostic, Span } from "./types";
  import { diagnosticCounts } from "./diagnostics";
  import { tooltip } from "./tooltip";
  import Icon from "./Icon.svelte";
  import DiagnosticsPanel from "./DiagnosticsPanel.svelte";
  import { themeIcon, type Theme } from "./theme";

  // The bottom status bar: a small, unobtrusive strip
  // hosting the dock toggle and a live problem indicator. It sets the
  // bottom-control styling the dock, diagnostics panel, and theme
  // switcher slot into.
  let {
    diagnostics = [],
    source = "",
    dockOpen = false,
    notice = null,
    autocomplete = true,
    formatOnSave = false,
    theme = "dark",
    onToggleDock,
    onToggleAutocomplete,
    onToggleFormatOnSave,
    onCycleTheme,
    onJumpToDiagnostic,
  }: {
    diagnostics?: Diagnostic[];
    // The active document's source, so the problems panel can show line/col and
    // the jump resolves spans against the right text.
    source?: string;
    dockOpen?: boolean;
    // A transient status flash (e.g. "Exported tune.pdf"); when set it takes the
    // diagnostics slot for a few seconds, then the caller clears it.
    notice?: string | null;
    // Editor autocomplete on/off (T7.24c): lit when on, muted when off.
    autocomplete?: boolean;
    // Format-on-save on/off (T7.25): when lit, every save canonicalizes first.
    formatOnSave?: boolean;
    // The colour theme (T7.26): the switcher cycles system → light → dark.
    theme?: Theme;
    onToggleDock?: () => void;
    onToggleAutocomplete?: () => void;
    onToggleFormatOnSave?: () => void;
    onCycleTheme?: () => void;
    // Jump the active editor's selection to a diagnostic's span (T7.28).
    onJumpToDiagnostic?: (span: Span) => void;
  } = $props();

  const counts = $derived(diagnosticCounts(diagnostics));
  const clean = $derived(counts.errors === 0 && counts.warnings === 0);

  // The problems panel: a popover above the problem button listing every
  // diagnostic; an entry click jumps the editor and closes the panel. Dismissed
  // on Escape or a pointer down outside the wrap. Force-closed when the document
  // goes clean (nothing left to list).
  let panelOpen = $state(false);
  $effect(() => {
    if (clean || notice) panelOpen = false;
  });
  $effect(() => {
    if (!panelOpen) return;
    function onPointer(e: PointerEvent) {
      const t = e.target;
      if (t instanceof Element && t.closest(".diag-wrap")) return;
      panelOpen = false;
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") panelOpen = false;
    }
    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("keydown", onKey);
    };
  });
  function jump(span: Span) {
    panelOpen = false;
    onJumpToDiagnostic?.(span);
  }
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
  <!-- The diagnostics/notice indicator stays pinned rightmost; any new control
       (theme, the toggles, future settings) goes to its left. -->
  <div class="group">
    <button
      class="control theme-toggle"
      aria-label="Theme: {theme}"
      use:tooltip={`Theme: ${theme}`}
      onclick={() => onCycleTheme?.()}
    >
      <Icon name={themeIcon(theme)} size={16} />
    </button>
    <button
      class="control format-toggle"
      class:active={formatOnSave}
      aria-label="Format on save: {formatOnSave ? 'on' : 'off'}"
      aria-pressed={formatOnSave}
      use:tooltip={`Format on save: ${formatOnSave ? "on" : "off"}`}
      onclick={() => onToggleFormatOnSave?.()}
    >
      <Icon name="text_format" size={16} />
    </button>
    <button
      class="control autocomplete-toggle"
      class:active={autocomplete}
      aria-label="Autocomplete: {autocomplete ? 'on' : 'off'}"
      aria-pressed={autocomplete}
      use:tooltip={`Autocomplete: ${autocomplete ? "on" : "off"}`}
      onclick={() => onToggleAutocomplete?.()}
    >
      <Icon name="prompt_suggestion" size={16} />
    </button>
    {#if notice}
      <!-- A transient export/success flash, taking the diagnostics slot. -->
      <div class="notice" role="status">
        <span class="ok" aria-hidden="true">✓</span>
        <span class="text">{notice}</span>
      </div>
    {:else if clean}
      <!-- No problems: a quiet, non-interactive indicator. -->
      <div class="diagnostics clean" use:tooltip={"No problems"}>
        <span class="ok" aria-hidden="true">✓</span>
        <span class="text">No problems</span>
      </div>
    {:else}
      <!-- Live problem counts: a button toggling the exhaustive problems panel. -->
      <div class="diag-wrap">
        {#if panelOpen}
          <div class="diag-popover">
            <DiagnosticsPanel
              {diagnostics}
              {source}
              onSelect={(entry) => jump(entry.span)}
            />
          </div>
        {/if}
        <button
          class="control diagnostics"
          class:active={panelOpen}
          aria-label="Problems: {counts.errors} error(s), {counts.warnings} warning(s)"
          aria-haspopup="listbox"
          aria-expanded={panelOpen}
          use:tooltip={`${counts.errors} error(s), ${counts.warnings} warning(s)`}
          onclick={() => (panelOpen = !panelOpen)}
        >
          <span class="count error">
            <span class="dot" aria-hidden="true">●</span>
            <span class="num">{counts.errors}</span>
          </span>
          <span class="count warning">
            <span class="dot" aria-hidden="true">▲</span>
            <span class="num">{counts.warnings}</span>
          </span>
        </button>
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
  /* The problem button anchors its panel; the popover floats above the bar,
     right-aligned to the viewport edge so it never overflows off-screen. */
  .diag-wrap {
    position: relative;
    display: inline-flex;
  }
  .diag-popover {
    position: absolute;
    bottom: 100%;
    right: 0;
    margin-bottom: 0.3rem;
    z-index: 1000;
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
