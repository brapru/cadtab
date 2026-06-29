import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import BottomBar from "./BottomBar.svelte";
import type { Diagnostic } from "./types";

function diag(
  severity: Diagnostic["severity"],
  start = 0,
  end = 1,
  message = "x",
): Diagnostic {
  return { severity, span: { start, end }, message, help: null };
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

  it("opens the problems panel from the count button and jumps on entry click", async () => {
    const onJumpToDiagnostic = vi.fn();
    const { container } = render(BottomBar, {
      source: "score {\n  3:0\n}",
      diagnostics: [diag("error", 10, 13, "boom")],
      onJumpToDiagnostic,
    });

    const button = container.querySelector("button.diagnostics")!;
    expect(button.getAttribute("aria-expanded")).toBe("false");
    expect(container.querySelector(".panel")).toBeNull();

    // Clicking the count button opens the exhaustive panel.
    await fireEvent.click(button);
    expect(button.getAttribute("aria-expanded")).toBe("true");
    const entry = container.querySelector(".panel .entry")!;
    expect(entry.querySelector(".msg")?.textContent).toBe("boom");

    // Clicking an entry jumps to its span and closes the panel.
    await fireEvent.click(entry);
    expect(onJumpToDiagnostic).toHaveBeenCalledWith({ start: 10, end: 13 });
    expect(container.querySelector(".panel")).toBeNull();
  });

  it("closes the panel on Escape", async () => {
    const { container } = render(BottomBar, {
      diagnostics: [diag("error")],
    });
    await fireEvent.click(container.querySelector("button.diagnostics")!);
    expect(container.querySelector(".panel")).not.toBeNull();

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(container.querySelector(".panel")).toBeNull();
  });

  it("has no problem button when clean", () => {
    const { container } = render(BottomBar, { diagnostics: [] });
    expect(container.querySelector("button.diagnostics")).toBeNull();
  });

  it("fires the help callback on click", async () => {
    const onOpenHelp = vi.fn();
    const { container } = render(BottomBar, { onOpenHelp });
    await fireEvent.click(container.querySelector(".help-toggle")!);
    expect(onOpenHelp).toHaveBeenCalledOnce();
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

  it("reflects the format-on-save setting and fires its toggle", async () => {
    const onToggleFormatOnSave = vi.fn();
    const { container, rerender } = render(BottomBar, {
      formatOnSave: false,
      onToggleFormatOnSave,
    });
    const toggle = container.querySelector(".format-toggle")!;
    // Off by default: muted, announced as not pressed.
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(toggle.classList.contains("active")).toBe(false);

    await fireEvent.click(toggle);
    expect(onToggleFormatOnSave).toHaveBeenCalledOnce();

    await rerender({ formatOnSave: true, onToggleFormatOnSave });
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    expect(toggle.getAttribute("aria-label")).toBe("Format on save: on");
    expect(toggle.classList.contains("active")).toBe(true);
  });

  it("shows the theme and cycles it on click", async () => {
    const onCycleTheme = vi.fn();
    const { container, rerender } = render(BottomBar, {
      theme: "dark",
      onCycleTheme,
    });
    const toggle = container.querySelector(".theme-toggle")!;
    expect(toggle.getAttribute("aria-label")).toBe("Theme: dark");

    await fireEvent.click(toggle);
    expect(onCycleTheme).toHaveBeenCalledOnce();

    await rerender({ theme: "light", onCycleTheme });
    expect(toggle.getAttribute("aria-label")).toBe("Theme: light");
  });
});
