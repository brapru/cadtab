<script lang="ts">
  // A small in-app confirmation modal, themed with the app tokens — cohesive with
  // the rest of the UI, and (unlike the native `window.confirm`) it works in the
  // desktop WKWebView. Purely presentational: the parent owns the open state and
  // settles its promise from `onConfirm`/`onCancel`.
  let {
    open,
    message,
    confirmLabel = "Confirm",
    cancelLabel = "Cancel",
    destructive = false,
    onConfirm,
    onCancel,
  }: {
    open: boolean;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    destructive?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  // Focus the confirm button when the dialog opens, so Enter/Space act on it and
  // the modal captures keyboard focus from whatever was behind it.
  let dialogEl = $state<HTMLDivElement | null>(null);
  let confirmEl = $state<HTMLButtonElement | null>(null);
  let cancelEl = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    if (open) confirmEl?.focus();
  });

  // The dialog's tabbable controls, in order (the backdrop is out of the tab
  // cycle), so Tab can be trapped between them.
  function focusables(): HTMLElement[] {
    return dialogEl
      ? [...dialogEl.querySelectorAll<HTMLElement>("button:not([disabled])")]
      : [];
  }

  // Trap Tab inside the dialog so focus can't wander to the chrome behind it:
  // wrap from the last control back to the first (and vice versa for Shift+Tab).
  function trapTab(e: KeyboardEvent) {
    const items = focusables();
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    const here = document.activeElement;
    if (e.shiftKey && (here === first || !dialogEl?.contains(here))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (here === last || !dialogEl?.contains(here))) {
      e.preventDefault();
      first.focus();
    }
  }

  // Escape cancels; Tab is trapped; Enter activates whichever button is focused
  // (so Enter on Cancel cancels, not confirms), defaulting to confirm when focus
  // rests on the dialog body. preventDefault avoids double-firing with the native
  // button activation.
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (document.activeElement === cancelEl) onCancel();
      else onConfirm();
    } else if (e.key === "Tab") {
      trapTab(e);
    }
  }
</script>

{#if open}
  <div class="overlay">
    <!-- A real button as the backdrop: clicking outside cancels, and it stays
         accessible (labelled) without a click-handler-on-a-div. Kept out of the
         tab cycle (Cancel/Escape already cover keyboard dismissal) so the trap
         only loops the dialog's own controls. -->
    <button
      class="backdrop"
      type="button"
      tabindex="-1"
      aria-label={cancelLabel}
      onclick={onCancel}
    ></button>
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-label={message}
      tabindex="-1"
      bind:this={dialogEl}
      onkeydown={onKey}
    >
      <p class="message">{message}</p>
      <div class="actions">
        <button class="btn cancel" bind:this={cancelEl} onclick={onCancel}
          >{cancelLabel}</button
        >
        <button
          class="btn confirm"
          class:destructive
          bind:this={confirmEl}
          onclick={onConfirm}>{confirmLabel}</button
        >
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  /* Fills the overlay behind the dialog; the dim is its background. */
  .backdrop {
    position: absolute;
    inset: 0;
    border: none;
    padding: 0;
    cursor: default;
    background: color-mix(in srgb, #000 45%, transparent);
  }
  .dialog {
    position: relative;
    min-width: 18rem;
    max-width: min(28rem, calc(100vw - 2rem));
    padding: 1.1rem 1.2rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--bg-chrome);
    color: var(--fg);
    /* A heavier lift than --shadow-popup: a centred modal over the dim backdrop
       reads as the topmost surface (T7.34d). */
    box-shadow: 0 0.5rem 1.5rem color-mix(in srgb, #000 30%, transparent);
  }
  .message {
    margin: 0 0 1rem;
    font-size: 0.9rem;
    line-height: 1.4;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
  .btn {
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    border-radius: 0.3rem;
    padding: 0.3rem 0.8rem;
    cursor: pointer;
    font-size: 0.85rem;
    line-height: 1;
  }
  .btn:hover {
    background: color-mix(in srgb, var(--fg) 6%, transparent);
  }
  /* Themed focus ring — replaces the browser's default blue outline. */
  .btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .confirm.destructive:focus-visible {
    outline-color: var(--error);
  }
  .confirm {
    border-color: var(--accent);
    color: var(--accent);
  }
  .confirm.destructive {
    border-color: var(--error);
    color: var(--error);
  }
  .confirm:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .confirm.destructive:hover {
    background: color-mix(in srgb, var(--error) 12%, transparent);
  }
</style>
