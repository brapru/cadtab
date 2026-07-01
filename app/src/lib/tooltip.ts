// A reusable hover/focus tooltip: `use:tooltip={content}` replaces native
// `title=` with a styled element portaled to <body>, so it is never clipped by an
// overflow:hidden/auto ancestor (the tab strip, the dock list). One tooltip shows
// at a time; the visual styling lives in app.css (`.app-tooltip`).
//
// `content` is either a plain string (a bare title) or a structured spec with an
// optional smaller description and a keyboard shortcut (T7.34f). The shortcut is
// a space-separated token chord — `mod`/`shift`/`alt` render platform-aware
// (⌘/⇧/⌥ on macOS, Ctrl/Shift/Alt elsewhere), any other token verbatim — drawn
// as key-cap `<kbd>` chips matching the Help view.

import { isMac } from "./menu";

export type TooltipContent =
  | string
  | { title: string; description?: string; shortcut?: string };

const mac = isMac();

// Map one chord token to its display label for the current platform.
function keyLabel(token: string): string {
  switch (token) {
    case "mod":
      return mac ? "⌘" : "Ctrl";
    case "shift":
      return mac ? "⇧" : "Shift";
    case "alt":
      return mac ? "⌥" : "Alt";
    default:
      return token;
  }
}

function spec(content: TooltipContent): {
  title: string;
  description?: string;
  shortcut?: string;
} {
  return typeof content === "string" ? { title: content } : content;
}

let current: HTMLElement | null = null;
let currentTarget: HTMLElement | null = null;

function removeTip() {
  current?.remove();
  current = null;
  currentTarget = null;
}

function buildTip(content: TooltipContent): HTMLElement | null {
  const { title, description, shortcut } = spec(content);
  if (!title) return null;
  const tip = document.createElement("div");
  tip.className = "app-tooltip";
  tip.setAttribute("role", "tooltip");

  // Title row: the bold title plus any shortcut chips, pushed to the right.
  const head = document.createElement("div");
  head.className = "tt-head";
  const titleEl = document.createElement("span");
  titleEl.className = "tt-title";
  titleEl.textContent = title;
  head.appendChild(titleEl);
  if (shortcut) {
    const keys = document.createElement("span");
    keys.className = "tt-keys";
    for (const token of shortcut.split(/\s+/).filter(Boolean)) {
      const kbd = document.createElement("kbd");
      kbd.textContent = keyLabel(token);
      keys.appendChild(kbd);
    }
    head.appendChild(keys);
  }
  tip.appendChild(head);

  if (description) {
    const desc = document.createElement("div");
    desc.className = "tt-desc";
    desc.textContent = description;
    tip.appendChild(desc);
  }
  return tip;
}

function showTip(target: HTMLElement, content: TooltipContent) {
  const tip = buildTip(content);
  if (!tip) return;
  removeTip();
  document.body.appendChild(tip);
  current = tip;
  currentTarget = target;
  // Horizontally centered, clamped to the viewport.
  const r = target.getBoundingClientRect();
  const left = Math.max(
    4,
    Math.min(
      r.left + r.width / 2 - tip.offsetWidth / 2,
      window.innerWidth - tip.offsetWidth - 4,
    ),
  );
  // Below the control by default; flip above when there isn't room below
  // (e.g. controls anchored on the bottom bar would otherwise clip off-screen).
  const gap = 6;
  const below = r.bottom + gap;
  const flipUp =
    below + tip.offsetHeight > window.innerHeight &&
    r.top - gap - tip.offsetHeight >= 0;
  tip.dataset.placement = flipUp ? "above" : "below";
  tip.style.top = `${flipUp ? r.top - gap - tip.offsetHeight : below}px`;
  tip.style.left = `${left}px`;
}

export function tooltip(node: HTMLElement, content: TooltipContent) {
  let value = content;
  const show = () => showTip(node, value);
  // Only this node's own tooltip should be torn down by its leave/blur.
  const hide = () => {
    if (currentTarget === node) removeTip();
  };
  node.addEventListener("pointerenter", show);
  node.addEventListener("pointerleave", hide);
  node.addEventListener("focusin", show);
  node.addEventListener("focusout", hide);
  // Activating the control (or starting a drag) dismisses its tooltip.
  node.addEventListener("pointerdown", hide);
  return {
    update(next: TooltipContent) {
      value = next;
      if (currentTarget === node) show(); // refresh a visible tooltip
    },
    destroy() {
      node.removeEventListener("pointerenter", show);
      node.removeEventListener("pointerleave", hide);
      node.removeEventListener("focusin", show);
      node.removeEventListener("focusout", hide);
      node.removeEventListener("pointerdown", hide);
      hide();
    },
  };
}
