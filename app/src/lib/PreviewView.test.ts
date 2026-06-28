import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import PreviewView from "./PreviewView.svelte";
import type { CompileResult } from "./types";

const result: CompileResult = {
  renderTree: {
    meta: { width: 12, height: 4 },
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
  diagnostics: [],
  tokens: [],
};

describe("PreviewView", () => {
  it("renders the export SVG (light sheet) for a compiled result", () => {
    const { container } = render(PreviewView, { result });
    const svg = container.querySelector(".sheet svg");
    expect(svg).not.toBeNull();
    // The export bakes a white page background and the document's title text.
    expect(
      container.querySelector('.sheet svg rect[fill="#ffffff"]'),
    ).not.toBeNull();
    expect(container.querySelector(".sheet svg text")?.textContent).toBe(
      "Cripple Creek",
    );
  });

  it("shows the error when there is no result", () => {
    const { container } = render(PreviewView, {
      result: null,
      error: "core unavailable",
    });
    expect(container.querySelector(".sheet")).toBeNull();
    expect(container.querySelector(".error")?.textContent).toBe(
      "core unavailable",
    );
  });

  it("fires onActivate on pointerdown (active-follows-focus)", async () => {
    const onActivate = vi.fn();
    const { container } = render(PreviewView, { result, onActivate });
    await fireEvent.pointerDown(container.querySelector(".preview")!);
    expect(onActivate).toHaveBeenCalled();
  });
});
