// A reusable hover/focus tooltip: `use:tooltip={text}` replaces native
// `title=` with a styled element portaled to <body>, so it is never clipped by an
// overflow:hidden/auto ancestor (the tab strip, the dock list). One tooltip shows
// at a time; the visual styling lives in app.css (`.app-tooltip`).

let current: HTMLElement | null = null;
let currentTarget: HTMLElement | null = null;

function removeTip() {
  current?.remove();
  current = null;
  currentTarget = null;
}

function showTip(target: HTMLElement, text: string) {
  if (!text) return;
  removeTip();
  const tip = document.createElement("div");
  tip.className = "app-tooltip";
  tip.setAttribute("role", "tooltip");
  tip.textContent = text;
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

export function tooltip(node: HTMLElement, text: string) {
  let label = text;
  const show = () => showTip(node, label);
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
    update(next: string) {
      label = next;
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
