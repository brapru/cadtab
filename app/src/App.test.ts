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
  basename: (p: string) => p.split(/[\\/]/).pop() || p,
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

  // Zoom is read off the rendered svg's --tab-zoom var (the in-pane % display was
  // removed in T7.12 — zoom lives on Cmd/Ctrl +/- and the tab-strip Fit control).
  function zoomOf(container: HTMLElement): number | null {
    const style =
      container.querySelector("svg.tab")?.getAttribute("style") ?? "";
    const m = style.match(/--tab-zoom:\s*([\d.]+)/);
    return m ? Number(m[1]) : null;
  }

  // The editor's code font scale, off the .editor container's inline font-size.
  function editorEm(container: HTMLElement): number | null {
    const style =
      container.querySelector(".editor")?.getAttribute("style") ?? "";
    const m = style.match(/font-size:\s*([\d.]+)em/);
    return m ? Number(m[1]) : null;
  }

  it("fits the focused render back to width from the tab-strip Fit control", async () => {
    const { container, getByLabelText } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).not.toBeNull();
    });
    expect(zoomOf(container)).toBe(1);

    // Focus the render so zoom targets it (the in-pane zoom toolbar is gone), then
    // zoom in via the keyboard.
    await fireEvent.pointerDown(container.querySelector(".render-side")!);
    await fireEvent.keyDown(window, { key: "=", ctrlKey: true });
    expect(zoomOf(container)).toBe(1.2);

    // The render group's Fit control resets to fit-width.
    await fireEvent.click(getByLabelText("Fit to width"));
    expect(zoomOf(container)).toBe(1);
  });

  it("zooms the focused render from Cmd/Ctrl +/- and fits with Cmd/Ctrl 0", async () => {
    const { container } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).not.toBeNull();
    });
    // Focus the render so the zoom keys target it, not the editor.
    await fireEvent.pointerDown(container.querySelector(".render-side")!);
    expect(zoomOf(container)).toBe(1);

    await fireEvent.keyDown(window, { key: "=", ctrlKey: true });
    expect(zoomOf(container)).toBe(1.2);

    await fireEvent.keyDown(window, { key: "-", ctrlKey: true });
    expect(zoomOf(container)).toBe(1);

    await fireEvent.keyDown(window, { key: "=", metaKey: true });
    expect(zoomOf(container)).toBe(1.2);

    await fireEvent.keyDown(window, { key: "0", metaKey: true });
    expect(zoomOf(container)).toBe(1);
  });

  it("zooms the focused editor's code font, leaving the render untouched", async () => {
    const { container } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector(".cm-content")).toBeTruthy();
    });
    // Focus the editor, then Cmd/Ctrl + grows the code font (not the render).
    await fireEvent.pointerDown(container.querySelector(".editor-pane")!);
    expect(editorEm(container)).toBe(1);

    await fireEvent.keyDown(window, { key: "=", ctrlKey: true });
    expect(editorEm(container)).toBe(1.2);
    expect(zoomOf(container)).toBe(1);

    // Cmd/Ctrl 0 returns the code font to its base size.
    await fireEvent.keyDown(window, { key: "0", ctrlKey: true });
    expect(editorEm(container)).toBe(1);
  });

  it("ignores +/- without a Cmd/Ctrl modifier", async () => {
    const { container } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).not.toBeNull();
    });
    await fireEvent.keyDown(window, { key: "=" });
    expect(zoomOf(container)).toBe(1);
    expect(editorEm(container)).toBe(1);
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

  it("opens a dock file as a new editor tab", async () => {
    openProjectMock.mockReset();
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
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      ),
    );

    // Reveal the dock and click the lib: it opens as its own focused editor tab.
    await fireEvent.click(container.querySelector(".dock-toggle")!);
    await fireEvent.click(getByText("lib.ctab"));
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "lib.ctab",
      );
      expect(container.querySelector(".cm-content")?.textContent).toContain(
        "def roll()",
      );
    });
  });

  it("makes the active document follow the focused editor tab", async () => {
    openProjectMock.mockReset();
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

    // Open the project, then a sibling lib from the dock: two editor tabs within
    // the one project (lib focused).
    await fireEvent.click(getByText("Open"));
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      ),
    );
    await fireEvent.click(container.querySelector(".dock-toggle")!);
    await fireEvent.click(getByText("lib.ctab"));
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "lib.ctab",
      ),
    );

    // Activating the entry's editor tab (the first one — the lib was appended
    // after it) makes it the active document again.
    const entryTab = [...container.querySelectorAll(".tab")].find((t) =>
      t.textContent?.includes("Editor"),
    )!;
    await fireEvent.click(entryTab);
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      ),
    );
  });

  it("opening a project closes the prior project's docs, tabs, and renders", async () => {
    openProjectMock.mockReset();
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
    const editorTabs = () =>
      [...container.querySelectorAll(".tab")].filter((t) =>
        t.textContent?.includes("Editor"),
      );

    // Open the bundle, then a dock lib — two editor tabs open in this project.
    await fireEvent.click(getByText("Open"));
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      ),
    );
    await fireEvent.click(container.querySelector(".dock-toggle")!);
    await fireEvent.click(getByText("lib.ctab"));
    await vi.waitFor(() => expect(editorTabs()).toHaveLength(2));

    // Open a different project: it replaces the prior one — a single editor tab
    // for the new score, the old project's tabs/docs gone.
    openProjectMock.mockResolvedValue({
      kind: "single",
      path: "/scores/other.ctab",
      name: "other.ctab",
      content: "score { 2:0 }",
    });
    await fireEvent.click(getByText("Open"));
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "other.ctab",
      );
      expect(editorTabs()).toHaveLength(1);
    });
    // The bundle's lib is no longer reachable from the dock (project reset).
    expect(screen.queryByText("lib.ctab")).toBeNull();
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

  it("marks the document dirty on edit; opening a project confirms then replaces it", async () => {
    openProjectMock.mockReset();
    openProjectMock.mockResolvedValue({
      kind: "single",
      path: "/x.ctab",
      name: "x.ctab",
      content: "score {}",
    });
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

    // Opening a project replaces the current one (T7.8); since the current doc is
    // dirty it raises our in-app confirm modal first. Accepting swaps in the
    // opened file as the sole doc.
    await fireEvent.keyDown(window, { key: "o", metaKey: true });
    let confirmBtn!: HTMLElement;
    await vi.waitFor(() => {
      confirmBtn = container.querySelector(".dialog .confirm")!;
      expect(confirmBtn).toBeTruthy();
    });
    // The native dialog is never consulted — this is our own DOM modal.
    await fireEvent.click(confirmBtn);
    await vi.waitFor(() => expect(openProjectMock).toHaveBeenCalled());
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "x.ctab",
      );
      // The modal closes once settled.
      expect(container.querySelector(".dialog")).toBeNull();
    });
    // The prior dirty doc is closed: a single editor tab for the opened file.
    const editorTabs = [...container.querySelectorAll(".tab-title")].filter(
      (t) => t.textContent === "Editor",
    );
    expect(editorTabs).toHaveLength(1);
  });

  it("keeps the current project when the discard prompt is cancelled", async () => {
    openProjectMock.mockReset();
    openProjectMock.mockResolvedValue({
      kind: "single",
      path: "/x.ctab",
      name: "x.ctab",
      content: "score {}",
    });
    const { container } = render(App);

    let content!: Element;
    await vi.waitFor(() => {
      content = container.querySelector(".cm-content")!;
      expect(content).toBeTruthy();
    });
    await fireEvent.keyDown(content, { key: "Tab" });
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name.dirty")).not.toBeNull(),
    );

    // Cancelling the modal aborts before the file picker — nothing is opened and
    // the dirty doc stays put.
    await fireEvent.keyDown(window, { key: "o", metaKey: true });
    let cancelBtn!: HTMLElement;
    await vi.waitFor(() => {
      cancelBtn = container.querySelector(".dialog .cancel")!;
      expect(cancelBtn).toBeTruthy();
    });
    await fireEvent.click(cancelBtn);
    expect(openProjectMock).not.toHaveBeenCalled();
    expect(container.querySelector(".dialog")).toBeNull();
    expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
      "untitled •",
    );
  });

  it("closes one view at a time, the session outliving its views (T7.11)", async () => {
    const { container, getByLabelText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    // Closing the render (a clean doc → no prompt) drops only that view; the
    // editor and the document's session remain.
    await fireEvent.click(getByLabelText("Close Render"));
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).toBeNull();
      expect(container.querySelector(".cm-content")).not.toBeNull();
    });
    expect(container.querySelector(".dialog")).toBeNull();
    expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
      "untitled",
    );

    // Closing the editor too removes the last view, emptying the layout.
    await fireEvent.click(getByLabelText("Close Editor"));
    await vi.waitFor(() =>
      expect(container.querySelector(".cm-content")).toBeNull(),
    );
    expect(container.querySelectorAll(".tab")).toHaveLength(0);
  });

  it("reopens a closed render from the active group's render control (T7.12)", async () => {
    const { container, getByLabelText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    // Close the render — it's gone, and the editor group's control set (now
    // active) invites reopening it.
    await fireEvent.click(getByLabelText("Close Render"));
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).toBeNull();
      expect(getByLabelText("Open render")).toBeTruthy();
    });

    // The control respawns the render for the document; it becomes the active
    // tab, so its Fit control now shows.
    await fireEvent.click(getByLabelText("Open render"));
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).not.toBeNull();
      expect(getByLabelText("Fit to width")).toBeTruthy();
    });
  });

  it("closing the editor leaves its render view open (T7.11)", async () => {
    const { container, getByLabelText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    // Views are independent: closing the (clean) editor keeps the render showing.
    await fireEvent.click(getByLabelText("Close Editor"));
    await vi.waitFor(() =>
      expect(container.querySelector(".cm-content")).toBeNull(),
    );
    expect(container.querySelector("svg.tab")).not.toBeNull();
    expect(container.querySelector(".dialog")).toBeNull();
  });

  it("guards the editor close of a dirty doc, then the final-view discard (T7.11)", async () => {
    const { container, getByLabelText } = render(App);
    let content!: Element;
    await vi.waitFor(() => {
      content = container.querySelector(".cm-content")!;
      expect(content).toBeTruthy();
    });
    await fireEvent.keyDown(content, { key: "Tab" });
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name.dirty")).not.toBeNull(),
    );

    // Closing the editor of a dirty doc warns; cancelling keeps it editable.
    await fireEvent.click(getByLabelText("Close Editor"));
    let cancelBtn!: HTMLElement;
    await vi.waitFor(() => {
      cancelBtn = container.querySelector(".dialog .cancel")!;
      expect(cancelBtn).toBeTruthy();
    });
    await fireEvent.click(cancelBtn);
    expect(container.querySelector(".cm-content")).not.toBeNull();

    // Confirming closes the editor; the render keeps the dirty doc on screen.
    await fireEvent.click(getByLabelText("Close Editor"));
    await vi.waitFor(() => {
      const confirm = container.querySelector<HTMLElement>(".dialog .confirm")!;
      expect(confirm).toBeTruthy();
    });
    await fireEvent.click(container.querySelector(".dialog .confirm")!);
    await vi.waitFor(() =>
      expect(container.querySelector(".cm-content")).toBeNull(),
    );
    expect(container.querySelector("svg.tab")).not.toBeNull();
    expect(container.querySelector(".doc-name.dirty")).not.toBeNull();

    // Closing the render is now the last view of a still-dirty doc: a final
    // discard prompt, after which the document is gone for good.
    await fireEvent.click(getByLabelText("Close Render"));
    await vi.waitFor(() => {
      const confirm = container.querySelector<HTMLElement>(".dialog .confirm")!;
      expect(confirm).toBeTruthy();
    });
    await fireEvent.click(container.querySelector(".dialog .confirm")!);
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).toBeNull();
      expect(container.querySelectorAll(".tab")).toHaveLength(0);
    });
  });

  it("reseeds an editor|render layout when New runs on an emptied workspace (T7.11)", async () => {
    const { container, getByLabelText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    // Empty the layout: close the render, then the (clean) editor.
    await fireEvent.click(getByLabelText("Close Render"));
    await fireEvent.click(getByLabelText("Close Editor"));
    await vi.waitFor(() =>
      expect(container.querySelectorAll(".tab")).toHaveLength(0),
    );

    // The empty-tabs placeholder still offers New; picking a template rebuilds
    // the editor|render split from scratch.
    await fireEvent.click(getByLabelText("New tab"));
    await fireEvent.click(screen.getByText("Guitar (standard)"));
    await vi.waitFor(() => {
      expect(container.querySelector(".cm-content")?.textContent).toContain(
        "instrument guitar",
      );
      expect(container.querySelector("svg.tab")).not.toBeNull();
    });
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

  it("opens a new document from the New + menu as its own untitled tab", async () => {
    const { container, getAllByLabelText } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector(".cm-content")).toBeTruthy();
    });

    // The tab-strip New "+" (on the active group) opens a template menu; picking
    // one opens a fresh untitled tab and focuses it.
    await fireEvent.click(getAllByLabelText("New tab")[0]);
    await fireEvent.click(screen.getByText("Guitar (standard)"));
    await vi.waitFor(() => {
      expect(container.querySelector(".cm-content")?.textContent).toContain(
        "instrument guitar",
      );
    });
    expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
      "untitled",
    );
    // The default document is still open alongside it (two editor tabs), with no
    // discard prompt — New never replaces the current doc.
    const editorTabs = [...container.querySelectorAll(".tab-title")].filter(
      (t) => t.textContent === "Editor",
    );
    expect(editorTabs).toHaveLength(2);
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

  it("opens the print preview as a tab showing the light export output", async () => {
    const { container, getByText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    await fireEvent.click(getByText("Preview"));

    // A Preview tab appears and renders the export SVG on a white sheet.
    await vi.waitFor(() => {
      const titles = [...container.querySelectorAll(".tab-title")].map(
        (t) => t.textContent,
      );
      expect(titles).toContain("Preview");
      expect(container.querySelector(".sheet svg")).not.toBeNull();
    });
    // The preview is the standalone export SVG, not the live themed render.
    expect(
      container.querySelector('.sheet svg rect[fill="#ffffff"]'),
    ).not.toBeNull();
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
    // The dock is collapsed by default, then revealed on the shortcut.
    expect(container.querySelector(".dock")).toBeNull();

    await fireEvent.keyDown(window, { key: "b", metaKey: true });
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    expect(container.querySelector(".dock")).not.toBeNull();

    await fireEvent.keyDown(window, { key: "b", ctrlKey: true });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(container.querySelector(".dock")).toBeNull();
  });

  it("lists an opened bundle's files in the project dock", async () => {
    openProjectMock.mockReset();
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
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      );
    });

    // Reveal the dock: it shows the entry (active) and the sibling lib, headed
    // by the bundle name.
    await fireEvent.click(container.querySelector(".dock-toggle")!);
    expect(container.querySelector(".dock-header")?.textContent).toBe(
      "proj.ctabz",
    );
    const names = [...container.querySelectorAll(".file-name")].map(
      (n) => n.textContent,
    );
    expect(names).toEqual(["lib.ctab", "tune.ctab"]);
    expect(
      container.querySelector(".file.active .file-name")?.textContent,
    ).toBe("tune.ctab");
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
