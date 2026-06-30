<script lang="ts">
  // The custom in-window titlebar (T7.45), desktop-only. Replaces the in-app
  // topbar on desktop now that Save/Export/Open already live in the native menu
  // (T7.30): the brand + open-project breadcrumb sit on the left, an Export
  // dropdown on the right. Save isn't here — ⌘S and the native menu cover it.
  // The whole bar is a drag region; on macOS the
  // native traffic lights overlay the left (the `titleBarStyle: Overlay` config
  // keeps them, content draws under), so we reserve space for them and skip our
  // own window buttons. Windows/Linux have no native frame (decorations dropped
  // in the Rust setup), so we paint min/maximize/close ourselves.
  import Icon from "./Icon.svelte";
  import { tooltip } from "./tooltip";

  let {
    // The open project/folder name, shown as `cadtab — <project>`; null shows
    // just the brand. NOT the filename — the active tab owns that.
    projectLabel = null,
    // macOS: reserve traffic-light space and hide our custom window buttons.
    mac = false,
    onExportSvg,
    onExportPng,
    onExportPdf,
    onExportBundle,
  }: {
    projectLabel?: string | null;
    mac?: boolean;
    onExportSvg: () => void;
    onExportPng: () => void;
    onExportPdf: () => void;
    onExportBundle: () => void;
  } = $props();

  // The Export dropdown, dismissed on Escape or a pointer down outside it
  // (mirrors the web topbar's export menu).
  let exportMenuOpen = $state(false);
  function chooseExport(fn: () => void) {
    exportMenuOpen = false;
    fn();
  }
  $effect(() => {
    if (!exportMenuOpen) return;
    function onPointer(e: PointerEvent) {
      const t = e.target;
      if (t instanceof Element && t.closest(".export-wrap")) return;
      exportMenuOpen = false;
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") exportMenuOpen = false;
    }
    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("keydown", onKey);
    };
  });

  // Window controls (Windows/Linux): the Tauri window API is imported lazily so
  // this component stays mountable under web/test where it has no runtime.
  async function winCtl(action: "min" | "max" | "close") {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const w = getCurrentWindow();
      if (action === "min") await w.minimize();
      else if (action === "max") await w.toggleMaximize();
      else await w.close();
    } catch (e) {
      console.error("window control failed:", e);
    }
  }
</script>

<!-- The bar (and its non-interactive regions) is the drag handle; the buttons
     opt out by simply not carrying the attribute. Decorative text is
     pointer-events:none so a press on it still drags the window. -->
<header class="titlebar" class:mac data-tauri-drag-region>
  <div class="left" class:mac data-tauri-drag-region>
    <span class="brand">cadtab</span>
    {#if projectLabel}<span class="project">— {projectLabel}</span>{/if}
  </div>
  <div class="spacer" data-tauri-drag-region></div>
  <div class="actions">
    <div class="export-wrap">
      <button
        class="text-btn"
        aria-label="Export"
        aria-haspopup="menu"
        aria-expanded={exportMenuOpen}
        use:tooltip={"Export the tab (SVG, PNG, PDF)"}
        onclick={() => (exportMenuOpen = !exportMenuOpen)}
      >
        Export
        <Icon name="arrow_drop_down" size={18} />
      </button>
      {#if exportMenuOpen}
        <div class="menu" role="menu">
          <button
            class="menu-item"
            role="menuitem"
            onclick={() => chooseExport(onExportSvg)}>Export SVG</button
          >
          <button
            class="menu-item"
            role="menuitem"
            onclick={() => chooseExport(onExportPng)}>Export PNG</button
          >
          <button
            class="menu-item"
            role="menuitem"
            onclick={() => chooseExport(onExportPdf)}>Export PDF</button
          >
          <button
            class="menu-item"
            role="menuitem"
            onclick={() => chooseExport(onExportBundle)}
            >Export Bundle (.ctabz)</button
          >
        </div>
      {/if}
    </div>
    {#if !mac}
      <div class="window-controls">
        <button
          class="win-btn"
          aria-label="Minimize"
          onclick={() => winCtl("min")}
        >
          <Icon name="remove" size={16} />
        </button>
        <button
          class="win-btn"
          aria-label="Maximize"
          onclick={() => winCtl("max")}
        >
          <Icon name="crop_square" size={13} />
        </button>
        <button
          class="win-btn close"
          aria-label="Close"
          onclick={() => winCtl("close")}
        >
          <Icon name="close" size={16} />
        </button>
      </div>
    {/if}
  </div>
</header>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    height: 38px;
    background: var(--bg-chrome);
    border-bottom: 1px solid var(--border);
    /* Window controls (Win/Linux) hug the top-right edge; mac gets a little
       breathing room past Export instead (see .titlebar.mac .actions). */
    padding-right: 0;
    user-select: none;
  }
  .left {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
    padding-left: 12px;
  }
  /* macOS: clear the native traffic lights overlaying the top-left. */
  .left.mac {
    padding-left: 78px;
  }
  .brand {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--fg);
    /* Let a press on the text drag the window (the bar is the drag region). */
    pointer-events: none;
  }
  .project {
    font-size: 0.82rem;
    color: var(--muted);
    pointer-events: none;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .actions {
    display: flex;
    align-items: center;
    height: 100%;
    gap: 0.15rem;
  }
  .titlebar.mac .actions {
    padding-right: 0.5rem;
  }
  /* Zed-style text controls: self-describing, flush, with a soft hover. */
  .text-btn {
    display: flex;
    align-items: center;
    gap: 0.05rem;
    border: none;
    background: transparent;
    color: var(--fg);
    font: inherit;
    font-size: 0.8rem;
    padding: 0.28rem 0.5rem;
    border-radius: 0.3rem;
    cursor: pointer;
    white-space: nowrap;
  }
  .text-btn:hover {
    background: color-mix(in srgb, var(--fg) 8%, transparent);
  }
  .export-wrap {
    position: relative;
    display: flex;
  }
  .menu {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 10;
    margin-top: 0.25rem;
    display: flex;
    flex-direction: column;
    min-width: 9rem;
    padding: 0.25rem;
    background: var(--bg-chrome);
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    box-shadow: var(--shadow-popup);
  }
  .menu-item {
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
  .menu-item:hover {
    background: color-mix(in srgb, var(--fg) 10%, transparent);
  }
  /* Windows/Linux caption buttons: full-height, edge-flush, the close button
     reddening on hover (the platform convention). */
  .window-controls {
    display: flex;
    align-items: stretch;
    height: 100%;
    margin-left: 0.3rem;
  }
  .win-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
  }
  .win-btn:hover {
    background: color-mix(in srgb, var(--fg) 10%, transparent);
    color: var(--fg);
  }
  .win-btn.close:hover {
    background: #e81123;
    color: #fff;
  }
</style>
