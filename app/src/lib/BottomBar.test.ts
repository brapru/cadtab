import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import BottomBar from "./BottomBar.svelte";
import type { Diagnostic } from "./types";

function diag(severity: Diagnostic["severity"]): Diagnostic {
  return { severity, span: { start: 0, end: 1 }, message: "x", help: null };
}

describe("BottomBar", () => {
  it("shows 'No problems' when there are no diagnostics", () => {
    const { container } = render(BottomBar, { diagnostics: [] });
    expect(container.querySelector(".diagnostics.clean")).not.toBeNull();
    expect(container.querySelector(".text")?.textContent).toBe("No problems");
  });

  it("shows error and warning counts when diagnostics are present", () => {
    const { container } = render(BottomBar, {
      diagnostics: [diag("error"), diag("error"), diag("warning")],
    });
    expect(container.querySelector(".diagnostics.clean")).toBeNull();
    expect(container.querySelector(".count.error .num")?.textContent).toBe("2");
    expect(container.querySelector(".count.warning .num")?.textContent).toBe(
      "1",
    );
  });

  it("reflects the dock state and fires the toggle on click", async () => {
    const onToggleDock = vi.fn();
    const { container, rerender } = render(BottomBar, {
      dockOpen: false,
      onToggleDock,
    });
    const toggle = container.querySelector(".dock-toggle")!;
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(toggle.classList.contains("active")).toBe(false);

    await fireEvent.click(toggle);
    expect(onToggleDock).toHaveBeenCalledOnce();

    // When the dock is open the toggle reads pressed.
    await rerender({ dockOpen: true, onToggleDock });
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    expect(toggle.classList.contains("active")).toBe(true);
  });

  it("reflects the autocomplete setting and fires its toggle on click", async () => {
    const onToggleAutocomplete = vi.fn();
    const { container, rerender } = render(BottomBar, {
      autocomplete: true,
      onToggleAutocomplete,
    });
    const toggle = container.querySelector(".autocomplete-toggle")!;
    // On by default: lit (active) and announced as pressed.
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    expect(toggle.getAttribute("aria-label")).toBe("Autocomplete: on");
    expect(toggle.classList.contains("active")).toBe(true);

    await fireEvent.click(toggle);
    expect(onToggleAutocomplete).toHaveBeenCalledOnce();

    // Off reads muted (not active) and updates its label.
    await rerender({ autocomplete: false, onToggleAutocomplete });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(toggle.getAttribute("aria-label")).toBe("Autocomplete: off");
    expect(toggle.classList.contains("active")).toBe(false);
  });
});
