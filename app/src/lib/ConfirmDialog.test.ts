import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import ConfirmDialog from "./ConfirmDialog.svelte";

function mount(props: Record<string, unknown> = {}) {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  const utils = render(ConfirmDialog, {
    open: true,
    message: "Discard unsaved changes?",
    onConfirm,
    onCancel,
    ...props,
  });
  return { ...utils, onConfirm, onCancel };
}

describe("ConfirmDialog", () => {
  it("renders nothing while closed", () => {
    const { container } = mount({ open: false });
    expect(container.querySelector(".dialog")).toBeNull();
  });

  it("shows the message and the supplied button labels when open", () => {
    const { container } = mount({
      confirmLabel: "Discard & Open",
      cancelLabel: "Keep editing",
    });
    expect(container.querySelector(".message")?.textContent).toBe(
      "Discard unsaved changes?",
    );
    expect(container.querySelector(".confirm")?.textContent?.trim()).toBe(
      "Discard & Open",
    );
    expect(container.querySelector(".cancel")?.textContent?.trim()).toBe(
      "Keep editing",
    );
  });

  it("fires onConfirm / onCancel from the respective buttons", async () => {
    const { container, onConfirm, onCancel } = mount();
    await fireEvent.click(container.querySelector(".confirm")!);
    expect(onConfirm).toHaveBeenCalledOnce();
    await fireEvent.click(container.querySelector(".cancel")!);
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("cancels on a backdrop click but not on a click inside the dialog", async () => {
    const { container, onCancel } = mount();
    await fireEvent.click(container.querySelector(".dialog")!);
    expect(onCancel).not.toHaveBeenCalled();
    await fireEvent.click(container.querySelector(".backdrop")!);
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("maps Escape to cancel; Enter activates the focused button", async () => {
    const { container, onConfirm, onCancel } = mount();
    const dialog = container.querySelector(".dialog")!;
    const cancel = container.querySelector<HTMLElement>(".cancel")!;
    const confirm = container.querySelector<HTMLElement>(".confirm")!;

    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledOnce();

    // Enter on the focused confirm button confirms...
    confirm.focus();
    await fireEvent.keyDown(dialog, { key: "Enter" });
    expect(onConfirm).toHaveBeenCalledOnce();

    // ...but Enter while Cancel is focused cancels — it does not confirm.
    cancel.focus();
    await fireEvent.keyDown(dialog, { key: "Enter" });
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("traps Tab within the dialog's controls", async () => {
    const { container } = mount();
    const dialog = container.querySelector(".dialog")!;
    const cancel = container.querySelector<HTMLElement>(".cancel")!;
    const confirm = container.querySelector<HTMLElement>(".confirm")!;

    // Confirm is focused on open; Tab from it wraps back to the first control.
    confirm.focus();
    await fireEvent.keyDown(dialog, { key: "Tab" });
    expect(document.activeElement).toBe(cancel);

    // Shift+Tab from the first control wraps to the last.
    cancel.focus();
    await fireEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(confirm);
  });

  it("keeps the backdrop out of the tab cycle", () => {
    const { container } = mount();
    expect(container.querySelector(".backdrop")?.getAttribute("tabindex")).toBe(
      "-1",
    );
  });

  it("flags the confirm button destructive when asked", () => {
    const { container } = mount({ destructive: true });
    expect(
      container.querySelector(".confirm")?.classList.contains("destructive"),
    ).toBe(true);
  });
});
