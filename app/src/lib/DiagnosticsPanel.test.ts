import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import DiagnosticsPanel from "./DiagnosticsPanel.svelte";
import type { Diagnostic } from "./types";

function diag(
  severity: Diagnostic["severity"],
  start: number,
  end: number,
  message: string,
  help: string | null = null,
): Diagnostic {
  return { severity, span: { start, end }, message, help };
}

const source = "score {\n  3:0\n}";

describe("DiagnosticsPanel", () => {
  it("lists each diagnostic in source order with message, help, and location", () => {
    const { container } = render(DiagnosticsPanel, {
      source,
      diagnostics: [
        diag("error", 10, 13, "second"),
        diag("warning", 0, 5, "first", "fix it"),
      ],
    });
    const entries = container.querySelectorAll(".entry");
    expect(entries).toHaveLength(2);

    // Sorted by position: the warning at byte 0 comes first.
    expect(entries[0].querySelector(".msg")?.textContent).toBe("first");
    expect(entries[0].querySelector(".help")?.textContent).toBe("fix it");
    expect(entries[0].querySelector(".loc")?.textContent?.trim()).toBe("1:1");
    expect(entries[0].querySelector(".icon")?.classList).toContain(
      "sev-warning",
    );

    // The error at byte 10 → line 2, col 3, no help line.
    expect(entries[1].querySelector(".msg")?.textContent).toBe("second");
    expect(entries[1].querySelector(".help")).toBeNull();
    expect(entries[1].querySelector(".loc")?.textContent?.trim()).toBe("2:3");
    expect(entries[1].querySelector(".icon")?.classList).toContain("sev-error");
  });

  it("calls onSelect with the entry when clicked", async () => {
    const onSelect = vi.fn();
    const { container } = render(DiagnosticsPanel, {
      source,
      diagnostics: [diag("error", 10, 13, "boom")],
      onSelect,
    });
    await fireEvent.click(container.querySelector(".entry")!);
    expect(onSelect).toHaveBeenCalledOnce();
    expect(onSelect.mock.calls[0][0]).toMatchObject({
      message: "boom",
      span: { start: 10, end: 13 },
    });
  });

  it("disables an unreachable (stale) entry so it can't be jumped to", async () => {
    const onSelect = vi.fn();
    const { container } = render(DiagnosticsPanel, {
      source: "abc",
      diagnostics: [diag("error", 50, 60, "stale")],
      onSelect,
    });
    const entry = container.querySelector(".entry") as HTMLButtonElement;
    expect(entry.disabled).toBe(true);
    expect(entry.querySelector(".loc")?.textContent?.trim()).toBe("—");
    await fireEvent.click(entry);
    expect(onSelect).not.toHaveBeenCalled();
  });
});
