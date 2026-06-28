<script lang="ts">
  import { tick } from "svelte";

  // A small pointer-positioned popup menu, fixed to the viewport so an
  // `overflow` ancestor (the dock list) can't clip it. Modeled on the New "+"
  // template popover: dismiss on Escape or a pointer down outside it. The host
  // owns open/close; this just renders `items` at (`x`, `y`) and reports a pick.
  export interface ContextMenuItem {
    label: string;
    action: string;
    destructive?: boolean;
    separatorBefore?: boolean;
  }

  let {
    x = 0,
    y = 0,
    items = [],
    onSelect,
    onDismiss,
  }: {
    x?: number;
    y?: number;
    items?: ContextMenuItem[];
    onSelect?: (action: string) => void;
    onDismiss?: () => void;
  } = $props();

  let menuEl = $state<HTMLDivElement | null>(null);
  let pos = $state({ left: 0, top: 0 });
  $effect(() => {
    // Place at the pointer immediately, then nudge back on-screen once measured.
    pos = { left: x, top: y };
    void tick().then(() => {
      if (!menuEl) return;
      const r = menuEl.getBoundingClientRect();
      const left = Math.min(x, window.innerWidth - r.width - 4);
      const top = Math.min(y, window.innerHeight - r.height - 4);
      pos = { left: Math.max(4, left), top: Math.max(4, top) };
    });
  });

  // Dismiss on Escape or a pointer down outside the menu.
  $effect(() => {
    function onPointer(e: PointerEvent) {
      const t = e.target;
      if (t instanceof Element && t.closest(".context-menu")) return;
      onDismiss?.();
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onDismiss?.();
    }
    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("keydown", onKey);
    };
  });
</script>

<div
  bind:this={menuEl}
  class="context-menu"
  role="menu"
  style="left: {pos.left}px; top: {pos.top}px"
>
  {#each items as item (item.action)}
    {#if item.separatorBefore}
      <div class="sep" role="separator"></div>
    {/if}
    <button
      class="item"
      class:destructive={item.destructive}
      role="menuitem"
      onclick={() => onSelect?.(item.action)}
    >
      {item.label}
    </button>
  {/each}
</div>

<style>
  .context-menu {
    position: fixed;
    z-index: 100;
    display: flex;
    flex-direction: column;
    min-width: 9rem;
    padding: 0.25rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    box-shadow: 0 6px 18px color-mix(in srgb, var(--fg) 18%, transparent);
  }
  .item {
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
  .item:hover {
    background: color-mix(in srgb, var(--fg) 10%, transparent);
  }
  .item.destructive {
    color: var(--error);
  }
  .item.destructive:hover {
    background: color-mix(in srgb, var(--error) 14%, transparent);
  }
  .sep {
    height: 1px;
    margin: 0.25rem 0.3rem;
    background: var(--border);
  }
</style>
