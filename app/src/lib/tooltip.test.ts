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
