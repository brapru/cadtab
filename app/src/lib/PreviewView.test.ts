import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import PreviewView from "./PreviewView.svelte";
import type { PaginatedTree } from "./types";

// PreviewView paginates its source through the core seam; mock that so the test
// drives the rendered pages directly.
const fakeTree: PaginatedTree = {
  pageWidth: 80,
  pageHeight: 103.5,
  pages: [
    {
      bounds: { x: 0, y: 0, w: 80, h: 103.5 },
      header: [
        {
          kind: "text",
          x: 1,
          y: 1,
          content: "Cripple Creek",
          role: "title",
          span: null,
        },
      ],
      systems: [],
    },
  ],
};
const paginateMock = vi.fn(async (..._args: unknown[]) => fakeTree);
vi.mock("./core", () => ({
  paginate: (...args: unknown[]) => paginateMock(...args),
}));

describe("PreviewView", () => {
  it("paginates the source and renders each page as a light sheet", async () => {
    const { container } = render(PreviewView, { source: "score { 3:0 }" });

    await vi.waitFor(() =>
      expect(container.querySelector(".sheet svg")).not.toBeNull(),
    );
    // Each page bakes a white sheet background and the document's header text.
    expect(
      container.querySelector('.sheet svg rect[fill="#ffffff"]'),
    ).not.toBeNull();
    expect(container.querySelector(".sheet svg text")?.textContent).toBe(
      "Cripple Creek",
    );
    // It paginated to the print page (Letter), not the screen layout.
    const [, config] = paginateMock.mock.calls[0] as [
      string,
      { size: string; contentWidth: number },
    ];
    expect(config.size).toBe("letter");
  });

  it("shows the error when there are no pages to show", async () => {
    paginateMock.mockRejectedValueOnce(new Error("no backend"));
    const { container } = render(PreviewView, {
      source: "x",
      error: "core unavailable",
    });
    // No pages → the backend error is surfaced instead.
    expect(container.querySelector(".sheet")).toBeNull();
    expect(container.querySelector(".error")?.textContent).toBe(
      "core unavailable",
    );
  });

  it("fires onActivate on pointerdown (active-follows-focus)", async () => {
    const onActivate = vi.fn();
    const { container } = render(PreviewView, { source: "x", onActivate });
    await fireEvent.pointerDown(container.querySelector(".preview")!);
    expect(onActivate).toHaveBeenCalled();
  });
});
