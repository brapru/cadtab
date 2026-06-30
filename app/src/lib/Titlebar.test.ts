import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import Titlebar from "./Titlebar.svelte";

// Minimal handler bag — each test overrides the ones it asserts on.
function handlers() {
  return {
    onExportSvg: vi.fn(),
    onExportPng: vi.fn(),
    onExportPdf: vi.fn(),
    onExportBundle: vi.fn(),
  };
}

describe("Titlebar", () => {
  it("shows the project breadcrumb when one is open, brand only otherwise", () => {
    const { container, rerender } = render(Titlebar, {
      projectLabel: "cripple-creek",
      ...handlers(),
    });
    expect(container.querySelector(".brand")?.textContent).toBe("cadtab");
    expect(container.querySelector(".project")?.textContent).toContain(
      "cripple-creek",
    );
    rerender({ projectLabel: null, ...handlers() });
    expect(container.querySelector(".project")).toBeNull();
  });

  it("opens the Export menu and dispatches each export", async () => {
    const h = handlers();
    const { getByLabelText, getByText, queryByRole } = render(Titlebar, {
      projectLabel: null,
      ...h,
    });
    // No menu until Export is clicked.
    expect(queryByRole("menu")).toBeNull();
    await fireEvent.click(getByLabelText("Export"));
    expect(queryByRole("menu")).not.toBeNull();

    // Picking an item dispatches it and closes the menu.
    await fireEvent.click(getByText("Export PDF"));
    expect(h.onExportPdf).toHaveBeenCalledOnce();
    expect(queryByRole("menu")).toBeNull();
  });

  it("paints custom window controls off macOS", () => {
    // Container-scoped (both renders share document.body).
    const { container } = render(Titlebar, {
      projectLabel: null,
      mac: false,
      ...handlers(),
    });
    const ctl = (label: string) =>
      container.querySelector(`[aria-label="${label}"]`);
    expect(ctl("Minimize")).not.toBeNull();
    expect(ctl("Maximize")).not.toBeNull();
    expect(ctl("Close")).not.toBeNull();
    expect(container.querySelector(".left.mac")).toBeNull();
  });

  it("uses native traffic lights on macOS — reserved space, no custom buttons", () => {
    const { container } = render(Titlebar, {
      projectLabel: null,
      mac: true,
      ...handlers(),
    });
    expect(container.querySelector('[aria-label="Minimize"]')).toBeNull();
    expect(container.querySelector('[aria-label="Close"]')).toBeNull();
    expect(container.querySelector(".left.mac")).not.toBeNull();
  });
});
