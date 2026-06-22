import { render, screen, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import type { CompileResult } from "./lib/types";

const fake: CompileResult = {
  renderTree: {
    meta: { width: 12, height: 4 },
    header: [],
    systems: [
      {
        bounds: { x: 0, y: 0, w: 12, h: 4 },
        prims: [],
        measures: [
          {
            bounds: { x: 0, y: 0, w: 12, h: 4 },
            prims: [
              {
                kind: "text",
                x: 1,
                y: 2,
                content: "0",
                role: "fretNumber",
                span: { start: 0, end: 5 },
              },
            ],
            span: null,
          },
        ],
      },
    ],
  },
  diagnostics: [
    {
      severity: "error",
      span: { start: 0, end: 5 },
      message: "bad",
      help: null,
    },
  ],
  tokens: [],
};

vi.mock("./lib/wasm", () => ({
  compile: vi.fn(async () => fake),
}));

import App from "./App.svelte";

describe("App", () => {
  it("renders the title heading", () => {
    render(App);
    expect(screen.getByRole("heading", { name: "cadtab" })).toBeInTheDocument();
  });

  it("renders the compiled tab via the wasm backend", async () => {
    const { container } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).not.toBeNull();
    });
    expect(container.querySelector("text")?.textContent).toBe("0");
  });

  it("renders the valid tab and surfaces diagnostics together (best-effort)", async () => {
    const { container } = render(App);
    await vi.waitFor(() => {
      // The render still shows valid parts even though the compile reported an
      // error, and the diagnostic is underlined in the editor.
      expect(container.querySelector("svg.tab")).not.toBeNull();
      expect(container.querySelector(".cm-diag-error")).not.toBeNull();
    });
  });

  it("round-trips a primitive click through selection back to an active highlight", async () => {
    const { container } = render(App);
    let fret!: Element;
    await vi.waitFor(() => {
      fret = container.querySelector('text[data-role="fretNumber"]')!;
      expect(fret).toBeTruthy();
    });

    // Clicking selects the note's source range in the editor; that selection
    // moves the cursor, which lights the same primitive back up.
    await fireEvent.click(fret);
    await vi.waitFor(() => {
      expect(
        container
          .querySelector('text[data-role="fretNumber"]')
          ?.classList.contains("active"),
      ).toBe(true);
    });
  });

  it("clears the highlight when clicking empty render space", async () => {
    const { container } = render(App);
    let fret!: Element;
    await vi.waitFor(() => {
      fret = container.querySelector('text[data-role="fretNumber"]')!;
      expect(fret).toBeTruthy();
    });

    await fireEvent.click(fret);
    await vi.waitFor(() => {
      expect(
        container
          .querySelector('text[data-role="fretNumber"]')
          ?.classList.contains("active"),
      ).toBe(true);
    });

    // A click on the pane background (not a primitive) drops the highlight.
    await fireEvent.click(container.querySelector(".render-pane")!);
    await vi.waitFor(() => {
      expect(
        container
          .querySelector('text[data-role="fretNumber"]')
          ?.classList.contains("active"),
      ).toBe(false);
    });
  });

  it("resizes the panes from the gutter arrow keys", async () => {
    const { container } = render(App);
    const gutter = container.querySelector('[role="slider"]')!;
    const editorPane = container.querySelector(".editor-pane") as HTMLElement;

    expect(gutter.getAttribute("aria-valuenow")).toBe("50");
    expect(editorPane.style.flex).not.toBe("");

    await fireEvent.keyDown(gutter, { key: "ArrowRight" });
    expect(gutter.getAttribute("aria-valuenow")).toBe("52");

    await fireEvent.keyDown(gutter, { key: "ArrowLeft" });
    await fireEvent.keyDown(gutter, { key: "ArrowLeft" });
    expect(gutter.getAttribute("aria-valuenow")).toBe("48");
  });

  it("tracks a pointer drag on the gutter", async () => {
    const { container } = render(App);
    const gutter = container.querySelector('[role="slider"]')!;

    await fireEvent.pointerDown(gutter, { pointerId: 1, clientX: 0 });
    expect(gutter.classList.contains("dragging")).toBe(true);
    await fireEvent.pointerMove(gutter, { pointerId: 1, clientX: 10 });
    await fireEvent.pointerUp(gutter, { pointerId: 1 });
    expect(gutter.classList.contains("dragging")).toBe(false);
  });

  it("zooms the render in and back to fit", async () => {
    const { container, getByLabelText } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).not.toBeNull();
    });
    const level = () => container.querySelector(".zoom-level")?.textContent;
    expect(level()).toBe("100%");

    await fireEvent.click(getByLabelText("Zoom in"));
    expect(level()).toBe("120%");
    expect(container.querySelector("svg.tab")?.getAttribute("style")).toContain(
      "--tab-zoom: 1.2",
    );

    await fireEvent.click(getByLabelText("Fit to width"));
    expect(level()).toBe("100%");
  });

  it("cycles the colour theme onto the document root", async () => {
    const { container } = render(App);
    const toggle = container.querySelector(".theme-toggle")!;
    const root = document.documentElement;

    expect(root.getAttribute("data-theme")).toBeNull(); // system
    await fireEvent.click(toggle);
    expect(root.getAttribute("data-theme")).toBe("light");
    await fireEvent.click(toggle);
    expect(root.getAttribute("data-theme")).toBe("dark");
    await fireEvent.click(toggle);
    expect(root.getAttribute("data-theme")).toBeNull(); // back to system
  });
});
