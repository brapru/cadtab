import { describe, it, expect, afterEach } from "vitest";
import { tooltip } from "./tooltip";

// jsdom has no PointerEvent constructor; a MouseEvent dispatched under the
// pointer event-type name triggers the listeners just the same.
function fire(node: Element, type: string) {
  node.dispatchEvent(new MouseEvent(type, { bubbles: true }));
}
function setup(text: string) {
  const node = document.createElement("button");
  document.body.appendChild(node);
  return { node, action: tooltip(node, text) };
}
function tip() {
  return document.querySelector(".app-tooltip");
}

describe("tooltip action", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("shows a styled, portaled tooltip on pointer enter and removes it on leave", () => {
    const { node } = setup("Save");
    fire(node, "pointerenter");
    const t = tip();
    expect(t?.textContent).toBe("Save");
    expect(t?.getAttribute("role")).toBe("tooltip");
    // Portaled to <body>, not nested in the control.
    expect(t?.parentElement).toBe(document.body);
    fire(node, "pointerleave");
    expect(tip()).toBeNull();
  });

  it("shows on focus and hides on blur", () => {
    const { node } = setup("Open");
    fire(node, "focusin");
    expect(tip()?.textContent).toBe("Open");
    fire(node, "focusout");
    expect(tip()).toBeNull();
  });

  it("dismisses on pointer down (activation)", () => {
    const { node } = setup("Fit");
    fire(node, "pointerenter");
    expect(tip()).not.toBeNull();
    fire(node, "pointerdown");
    expect(tip()).toBeNull();
  });

  it("refreshes a visible tooltip when its label changes", () => {
    const { node, action } = setup("Maximize");
    fire(node, "pointerenter");
    expect(tip()?.textContent).toBe("Maximize");
    action.update("Restore");
    expect(tip()?.textContent).toBe("Restore");
  });

  it("shows nothing for empty text", () => {
    const { node } = setup("");
    fire(node, "pointerenter");
    expect(tip()).toBeNull();
  });

  it("flips above the control when there isn't room below", () => {
    const original = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "offsetHeight",
    );
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      get: () => 30,
    });
    try {
      const { node } = setup("Fit");
      // Anchored at the bottom of the viewport (innerHeight 768 in jsdom).
      node.getBoundingClientRect = () =>
        ({ top: 740, bottom: 760, left: 100, width: 40 }) as DOMRect;
      fire(node, "pointerenter");
      const t = tip() as HTMLElement;
      expect(t.dataset.placement).toBe("above");
      // top = r.top - gap(6) - height(30).
      expect(t.style.top).toBe("704px");
    } finally {
      if (original)
        Object.defineProperty(HTMLElement.prototype, "offsetHeight", original);
    }
  });

  it("stays below the control when there is room", () => {
    const original = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "offsetHeight",
    );
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      get: () => 30,
    });
    try {
      const { node } = setup("Fit");
      node.getBoundingClientRect = () =>
        ({ top: 20, bottom: 40, left: 100, width: 40 }) as DOMRect;
      fire(node, "pointerenter");
      const t = tip() as HTMLElement;
      expect(t.dataset.placement).toBe("below");
      // top = r.bottom + gap(6).
      expect(t.style.top).toBe("46px");
    } finally {
      if (original)
        Object.defineProperty(HTMLElement.prototype, "offsetHeight", original);
    }
  });

  it("renders a structured tooltip: bold title, description, and shortcut chips", () => {
    const node = document.createElement("button");
    document.body.appendChild(node);
    tooltip(node, {
      title: "Save",
      description: "Write the active file",
      shortcut: "mod S",
    });
    fire(node, "pointerenter");
    const t = tip()!;
    expect(t.querySelector(".tt-title")?.textContent).toBe("Save");
    expect(t.querySelector(".tt-desc")?.textContent).toBe(
      "Write the active file",
    );
    const caps = [...t.querySelectorAll("kbd")].map((k) => k.textContent);
    // The "mod" token renders platform-aware (⌘ on macOS, Ctrl elsewhere); the
    // literal "S" passes through unchanged.
    expect(caps).toHaveLength(2);
    expect(["⌘", "Ctrl"]).toContain(caps[0]);
    expect(caps[1]).toBe("S");
  });

  it("omits the description and shortcut when not provided", () => {
    const node = document.createElement("button");
    document.body.appendChild(node);
    tooltip(node, { title: "New tab" });
    fire(node, "pointerenter");
    const t = tip()!;
    expect(t.querySelector(".tt-title")?.textContent).toBe("New tab");
    expect(t.querySelector(".tt-desc")).toBeNull();
    expect(t.querySelector(".tt-keys")).toBeNull();
  });

  it("tears down listeners and any tooltip on destroy", () => {
    const { node, action } = setup("Close");
    fire(node, "pointerenter");
    expect(tip()).not.toBeNull();
    action.destroy();
    expect(tip()).toBeNull();
    // No tooltip reappears after destroy.
    fire(node, "pointerenter");
    expect(tip()).toBeNull();
  });
});
