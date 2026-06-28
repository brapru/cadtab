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

const wasmCompileMock = vi.fn(async (..._args: unknown[]) => fake);
vi.mock("./lib/wasm", () => ({
  compile: (...args: unknown[]) => wasmCompileMock(...args),
}));

const openProjectMock = vi.fn();
const saveDocumentMock = vi.fn();
const saveBundleMock = vi.fn();
const saveSvgMock = vi.fn();
const savePngMock = vi.fn();
vi.mock("./lib/io", () => ({
  openProject: (...args: unknown[]) => openProjectMock(...args),
  saveDocument: (...args: unknown[]) => saveDocumentMock(...args),
  saveBundle: (...args: unknown[]) => saveBundleMock(...args),
  saveSvg: (...args: unknown[]) => saveSvgMock(...args),
  savePng: (...args: unknown[]) => savePngMock(...args),
  defaultDocName: () => "untitled.ctab",
}));

const svgToPngBlobMock = vi.fn(
  async (..._args: unknown[]) => new Blob(["png"], { type: "image/png" }),
);
vi.mock("./lib/png", () => ({
  svgToPngBlob: (...args: unknown[]) => svgToPngBlobMock(...args),
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

  it("resizes the groups from the gutter arrow keys", async () => {
    const { container } = render(App);
    const gutter = container.querySelector('[role="slider"]')!;
    const leftGroup = container.querySelector(".group") as HTMLElement;

    expect(gutter.getAttribute("aria-valuenow")).toBe("50");
    expect(leftGroup.style.flex).not.toBe("");

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

  it("opens a single score, shows its name, and stays clean", async () => {
    openProjectMock.mockReset();
    openProjectMock.mockResolvedValue({
      kind: "single",
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

  it("opens a project bundle: loads the entry and passes libs to compile", async () => {
    openProjectMock.mockReset();
    wasmCompileMock.mockClear();
    openProjectMock.mockResolvedValue({
      kind: "bundle",
      path: "/proj.ctabz",
      name: "proj.ctabz",
      bundle: {
        entry: "tune.ctab",
        files: {
          "tune.ctab": 'import "lib.ctab"\nscore { roll() }',
          "lib.ctab": "def roll() { 3:0 }",
        },
      },
    });
    const { container, getByText } = render(App);

    await fireEvent.click(getByText("Open"));

    // The entry becomes the open document...
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      );
    });
    // ...and the sibling lib (not the entry) flows to compile as the bundle map.
    await vi.waitFor(() => {
      const libsArgs = wasmCompileMock.mock.calls.map((c) => c[2]);
      expect(libsArgs).toContainEqual({ "lib.ctab": "def roll() { 3:0 }" });
    });
  });

  it("saves an opened file in place, reusing its path (no re-prompt)", async () => {
    openProjectMock.mockReset();
    saveDocumentMock.mockReset();
    openProjectMock.mockResolvedValue({
      kind: "single",
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
    await vi.waitFor(() => expect(openProjectMock).toHaveBeenCalled());

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
    openProjectMock.mockReset();
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
    expect(openProjectMock).not.toHaveBeenCalled();

    confirmSpy.mockReturnValue(true);
    openProjectMock.mockResolvedValue({
      kind: "single",
      path: "/x.ctab",
      name: "x.ctab",
      content: "score {}",
    });
    await fireEvent.keyDown(window, { key: "o", metaKey: true });
    await vi.waitFor(() => expect(openProjectMock).toHaveBeenCalled());
    confirmSpy.mockRestore();
  });

  it("saves a project bundle from the Save Project button", async () => {
    saveBundleMock.mockReset();
    saveBundleMock.mockResolvedValue({
      path: "/proj.ctabz",
      name: "proj.ctabz",
    });
    const { getByText } = render(App);

    await fireEvent.click(getByText("Save Project"));

    await vi.waitFor(() => expect(saveBundleMock).toHaveBeenCalled());
    // The bundle carries the entry name plus the live editor source under it.
    const [bundle] = saveBundleMock.mock.calls[0] as [
      { entry: string; files: Record<string, string> },
    ];
    expect(bundle.entry).toBe("untitled.ctab");
    expect(bundle.files["untitled.ctab"]).toContain("Cripple Creek");
  });

  it("exports the rendered tab as a standalone SVG", async () => {
    saveSvgMock.mockReset();
    saveSvgMock.mockResolvedValue({ path: null, name: "untitled.svg" });
    const { container, getByText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    await fireEvent.click(getByText("Export SVG"));

    await vi.waitFor(() => expect(saveSvgMock).toHaveBeenCalled());
    const [svg, target] = saveSvgMock.mock.calls[0] as [
      string,
      { path: string | null; suggestedName: string },
    ];
    // The real serializer ran on the current render tree.
    expect(svg).toContain("<svg");
    expect(target).toEqual({ path: null, suggestedName: "untitled.ctab" });
  });

  it("exports the rendered tab as a PNG via the rasterizer", async () => {
    savePngMock.mockReset();
    svgToPngBlobMock.mockClear();
    savePngMock.mockResolvedValue({ path: null, name: "untitled.png" });
    const { container, getByText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    await fireEvent.click(getByText("Export PNG"));

    await vi.waitFor(() => expect(savePngMock).toHaveBeenCalled());
    // The SVG was rasterized to a blob before saving.
    expect(svgToPngBlobMock).toHaveBeenCalled();
    const [blob] = savePngMock.mock.calls[0] as [Blob];
    expect(blob.type).toBe("image/png");
  });

  it("starts a new document from a template, guarding unsaved edits", async () => {
    const { container, getByLabelText } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector(".cm-content")).toBeTruthy();
    });

    const select = getByLabelText("New from template") as HTMLSelectElement;

    // A clean document → New loads the chosen template as an untitled doc.
    await fireEvent.change(select, { target: { value: "guitar" } });
    await vi.waitFor(() => {
      expect(container.querySelector(".cm-content")?.textContent).toContain(
        "instrument guitar",
      );
    });
    expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
      "untitled",
    );

    // Edit, then a declined New keeps the current document.
    await fireEvent.keyDown(container.querySelector(".cm-content")!, {
      key: "Tab",
    });
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name.dirty")).not.toBeNull();
    });
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    await fireEvent.change(select, { target: { value: "blank" } });
    expect(container.querySelector(".cm-content")?.textContent).toContain(
      "instrument guitar",
    );
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

  it("surfaces the compile's diagnostics in the bottom bar", async () => {
    const { container } = render(App);
    // The fake compile reports one error, so the bottom bar leaves the clean
    // state and shows an error count of 1.
    await vi.waitFor(() => {
      expect(container.querySelector(".diagnostics.clean")).toBeNull();
      expect(container.querySelector(".count.error .num")?.textContent).toBe(
        "1",
      );
    });
  });

  it("toggles the project dock from Cmd/Ctrl-B", async () => {
    const { container } = render(App);
    const toggle = container.querySelector(".dock-toggle")!;
    expect(toggle.getAttribute("aria-pressed")).toBe("false");

    await fireEvent.keyDown(window, { key: "b", metaKey: true });
    expect(toggle.getAttribute("aria-pressed")).toBe("true");

    await fireEvent.keyDown(window, { key: "b", ctrlKey: true });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
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
