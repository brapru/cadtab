<script lang="ts">
  import type { Diagnostic } from "./types";
  import { diagnosticEntries, type DiagnosticEntry } from "./diagnostics";
  import Icon from "./Icon.svelte";

  // The exhaustive problems list (T7.28): every diagnostic for the active
  // document, sorted by source position. Clicking an entry jumps the editor's
  // selection to its span. Rendered as a popover above the bottom bar's problem
  // button; the bar owns open/dismiss and the jump wiring.
  let {
    diagnostics = [],
    source = "",
    onSelect,
  }: {
    diagnostics?: Diagnostic[];
    source?: string;
    onSelect?: (entry: DiagnosticEntry) => void;
  } = $props();

  const entries = $derived(diagnosticEntries(source, diagnostics));

  // Material Symbol per severity (filled, coloured by the semantic token).
  const icons: Record<DiagnosticEntry["severity"], string> = {
    error: "error",
    warning: "warning",
    info: "info",
  };
</script>

<div class="panel" role="listbox" aria-label="Problems">
  <div class="head">Problems</div>
  <ul class="list">
    {#each entries as entry (entry.span.start + ":" + entry.span.end + ":" + entry.message)}
      <li>
        <button
          type="button"
          class="entry"
          class:unreachable={!entry.inRange}
          role="option"
          aria-selected="false"
          disabled={!entry.inRange}
          onclick={() => entry.inRange && onSelect?.(entry)}
        >
          <span class="icon sev-{entry.severity}" aria-hidden="true">
            <Icon name={icons[entry.severity]} size={15} fill={true} />
          </span>
          <span class="body">
            <span class="msg">{entry.message}</span>
            {#if entry.help}<span class="help">{entry.help}</span>{/if}
          </span>
          <span class="loc" aria-hidden="true">
            {entry.inRange ? `${entry.line}:${entry.col}` : "—"}
          </span>
        </button>
      </li>
    {/each}
  </ul>
</div>

<style>
  .panel {
    width: min(28rem, 80vw);
    max-height: 16rem;
    display: flex;
    flex-direction: column;
    background: var(--bg-chrome);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    box-shadow: var(--shadow-popup);
    overflow: hidden;
    font-size: 0.75rem;
  }
  .head {
    padding: 0.3rem 0.6rem;
    color: var(--muted);
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    border-bottom: 1px solid var(--border);
  }
  .list {
    margin: 0;
    padding: 0.2rem;
    list-style: none;
    overflow-y: auto;
  }
  .entry {
    display: flex;
    align-items: flex-start;
    gap: 0.45rem;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    padding: 0.3rem 0.4rem;
    border-radius: 0.3rem;
    cursor: pointer;
  }
  .entry:hover,
  .entry:focus-visible {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    outline: none;
  }
  .entry.unreachable {
    cursor: default;
    opacity: 0.6;
  }
  .entry.unreachable:hover {
    background: transparent;
  }
  .icon {
    flex: 0 0 auto;
    display: inline-flex;
    line-height: 1;
    margin-top: 0.05rem;
  }
  .icon.sev-error {
    color: var(--error);
  }
  .icon.sev-warning {
    color: var(--warning);
  }
  .icon.sev-info {
    color: var(--info);
  }
  .body {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
  }
  .msg {
    color: var(--fg);
    line-height: 1.35;
  }
  .help {
    color: var(--muted);
    font-size: 0.92em;
    line-height: 1.3;
  }
  .loc {
    flex: 0 0 auto;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    font-size: 0.92em;
    margin-top: 0.05rem;
  }
</style>
