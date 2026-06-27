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

const openDocumentMock = vi.fn();
const saveDocumentMock = vi.fn();
vi.mock("./lib/io", () => ({
  openDocument: (...args: unknown[]) => openDocumentMock(...args),
  saveDocument: (...args: unknown[]) => saveDocumentMock(...args),
  defaultDocName: () => "untitled.ctab",
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

  it("zooms from Cmd/Ctrl +/- and fits with Cmd/Ctrl 0", async () => {
    const { container } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).not.toBeNull();
    });
    const level = () => container.querySelector(".zoom-level")?.textContent;
    expect(level()).toBe("100%");

    await fireEvent.keyDown(window, { key: "=", ctrlKey: true });
    expect(level()).toBe("120%");

    await fireEvent.keyDown(window, { key: "-", ctrlKey: true });
    expect(level()).toBe("100%");

    await fireEvent.keyDown(window, { key: "=", metaKey: true });
    expect(level()).toBe("120%");

    await fireEvent.keyDown(window, { key: "0", metaKey: true });
    expect(level()).toBe("100%");
  });

  it("ignores +/- without a Cmd/Ctrl modifier", async () => {
    const { container } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).not.toBeNull();
    });
    const level = () => container.querySelector(".zoom-level")?.textContent;
    await fireEvent.keyDown(window, { key: "=" });
    expect(level()).toBe("100%");
  });

  it("opens a document, shows its name, and stays clean", async () => {
    openDocumentMock.mockReset();
    openDocumentMock.mockResolvedValue({
      path: "/scores/loaded.ctab",
      name: "loaded.ctab",
      content: "score { 1:0 }",
    });
    const { container, getByText } = render(App);

    await fireEvent.click(getByText("Open"));

    await vi.waitFor(() => {
      const name = container.querySelector(".doc-name");
      expect(name?.textContent?.trim()).toBe("loaded.ctab");
      // The freshly loaded doc is not a user edit, so it is not marked dirty.
      expect(container.querySelector(".doc-name.dirty")).toBeNull();
    });
  });

  it("saves an opened file in place, reusing its path (no re-prompt)", async () => {
    openDocumentMock.mockReset();
    saveDocumentMock.mockReset();
    openDocumentMock.mockResolvedValue({
      path: "/scores/loaded.ctab",
      name: "loaded.ctab",
      content: "score { 1:0 }",
    });
    saveDocumentMock.mockResolvedValue({
      path: "/scores/loaded.ctab",
      name: "loaded.ctab",
    });
    const { getByText } = render(App);

    await fireEvent.click(getByText("Open"));
    await vi.waitFor(() => expect(openDocumentMock).toHaveBeenCalled());

    await fireEvent.click(getByText("Save"));
    await vi.waitFor(() => expect(saveDocumentMock).toHaveBeenCalled());
    // Save targets the opened path, so the backend overwrites in place.
    const [, target] = saveDocumentMock.mock.calls[0] as [
      string,
      { path: string | null; suggestedName: string },
    ];
    expect(target.path).toBe("/scores/loaded.ctab");
  });

  it("saves the current source and adopts the saved name", async () => {
    saveDocumentMock.mockReset();
    saveDocumentMock.mockResolvedValue({
      path: "/x/tune.ctab",
      name: "tune.ctab",
    });
    const { container, getByText } = render(App);

    await fireEvent.click(getByText("Save"));

    await vi.waitFor(() => {
      expect(saveDocumentMock).toHaveBeenCalled();
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      );
    });
    // The default doc has no path yet, so save targets a dialog seeded by the
    // title-derived name, and sends the current editor source.
    const [content, target] = saveDocumentMock.mock.calls[0] as [
      string,
      { path: string | null; suggestedName: string },
    ];
    expect(content).toContain("Cripple Creek");
    expect(target).toEqual({ path: null, suggestedName: "untitled.ctab" });
  });

  it("saves from the Cmd/Ctrl+S shortcut", async () => {
    saveDocumentMock.mockReset();
    saveDocumentMock.mockResolvedValue({
      path: "/x/tune.ctab",
      name: "tune.ctab",
    });
    render(App);

    await fireEvent.keyDown(window, { key: "s", metaKey: true });
    await vi.waitFor(() => expect(saveDocumentMock).toHaveBeenCalled());
  });

  it("marks the document dirty on edit and guards an unsaved open", async () => {
    openDocumentMock.mockReset();
    const { container } = render(App);

    let content!: Element;
    await vi.waitFor(() => {
      content = container.querySelector(".cm-content")!;
      expect(content).toBeTruthy();
    });

    // Editing (Tab indents) marks the doc dirty after the debounced compile.
    await fireEvent.keyDown(content, { key: "Tab" });
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name.dirty")).not.toBeNull();
    });

    // Declining the discard prompt aborts the open; accepting proceeds.
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    await fireEvent.keyDown(window, { key: "o", metaKey: true });
    expect(openDocumentMock).not.toHaveBeenCalled();

    confirmSpy.mockReturnValue(true);
    openDocumentMock.mockResolvedValue({
      path: "/x.ctab",
      name: "x.ctab",
      content: "score {}",
    });
    await fireEvent.keyDown(window, { key: "o", metaKey: true });
    await vi.waitFor(() => expect(openDocumentMock).toHaveBeenCalled());
    confirmSpy.mockRestore();
  });

  it("clears dirty when edits are undone back to the saved baseline", async () => {
    const { container } = render(App);

    let content!: Element;
    await vi.waitFor(() => {
      content = container.querySelector(".cm-content")!;
      expect(content).toBeTruthy();
    });

    await fireEvent.keyDown(content, { key: "Tab" });
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name.dirty")).not.toBeNull();
    });

    // Undoing the edit returns the buffer to the baseline, so it is clean again.
    await fireEvent.keyDown(content, { key: "z", ctrlKey: true });
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name.dirty")).toBeNull();
    });
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
