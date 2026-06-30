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

const fakePaginated = {
  pageWidth: 80,
  pageHeight: 103.5,
  pages: [{ bounds: { x: 0, y: 0, w: 80, h: 103.5 }, header: [], systems: [] }],
};
const wasmCompileMock = vi.fn(async (..._args: unknown[]) => fake);
const wasmPaginateMock = vi.fn(async (..._args: unknown[]) => fakePaginated);
const wasmCompletionsMock = vi.fn(async (..._args: unknown[]) => ({
  keywords: [],
  identifiers: [],
}));
// The formatter echoes a sentinel so a test can see the formatted text land.
const wasmFormatMock = vi.fn(async (_source: string) => 'title "Formatted"\n');
vi.mock("./lib/wasm", () => ({
  compile: (...args: unknown[]) => wasmCompileMock(...args),
  paginate: (...args: unknown[]) => wasmPaginateMock(...args),
  completions: (...args: unknown[]) => wasmCompletionsMock(...args),
  format: (...args: unknown[]) => wasmFormatMock(...(args as [string])),
}));

const openProjectMock = vi.fn();
const openFolderMock = vi.fn();
const rescanFolderMock = vi.fn();
const createFileMock = vi.fn();
const createDirMock = vi.fn();
const removePathMock = vi.fn();
const renamePathMock = vi.fn();
const saveDocumentMock = vi.fn();
const saveBundleMock = vi.fn();
const saveSvgMock = vi.fn();
const savePngMock = vi.fn();
const savePdfMock = vi.fn();
// Capture the watch callback so a test can fire a synthetic fs change.
let watchCallback: (() => void) | null = null;
const unwatchMock = vi.fn();
vi.mock("./lib/io", () => ({
  openProject: (...args: unknown[]) => openProjectMock(...args),
  openFolder: (...args: unknown[]) => openFolderMock(...args),
  rescanFolder: (...args: unknown[]) => rescanFolderMock(...args),
  watchFolder: async (_root: string, cb: () => void) => {
    watchCallback = cb;
    return unwatchMock;
  },
  createFile: (...args: unknown[]) => createFileMock(...args),
  createDir: (...args: unknown[]) => createDirMock(...args),
  removePath: (...args: unknown[]) => removePathMock(...args),
  renamePath: (...args: unknown[]) => renamePathMock(...args),
  saveDocument: (...args: unknown[]) => saveDocumentMock(...args),
  saveBundle: (...args: unknown[]) => saveBundleMock(...args),
  saveSvg: (...args: unknown[]) => saveSvgMock(...args),
  savePng: (...args: unknown[]) => savePngMock(...args),
  savePdf: (...args: unknown[]) => savePdfMock(...args),
  defaultDocName: () => "untitled.ctab",
  basename: (p: string) => p.split(/[\\/]/).pop() || p,
  resolvePath: (root: string, key: string) => `${root}/${key}`,
  withCtabExtension: (n: string) =>
    n.toLowerCase().endsWith(".ctab") ? n : `${n}.ctab`,
}));

// The desktop backend invokes a Tauri command to compile; stub it so a
// desktop-mode render (with __TAURI_INTERNALS__ set) still produces a render.
const invokeMock = vi.fn(async (..._args: unknown[]) => fake);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// Toggle the Tauri-webview marker `isTauri()` keys off, so a test can render the
// app in desktop mode. Cleaned up per-test so others stay in web mode.
function setDesktop(on: boolean) {
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  if (on) w.__TAURI_INTERNALS__ = {};
  else delete w.__TAURI_INTERNALS__;
}

const svgToPngBlobMock = vi.fn(
  async (..._args: unknown[]) => new Blob(["png"], { type: "image/png" }),
);
vi.mock("./lib/png", () => ({
  svgToPngBlob: (...args: unknown[]) => svgToPngBlobMock(...args),
}));

const paginatedTreeToPdfMock = vi.fn(
  async (..._args: unknown[]) => new Uint8Array([37, 80, 68, 70]),
);
vi.mock("./lib/pdf", () => ({
  paginatedTreeToPdf: (...args: unknown[]) => paginatedTreeToPdfMock(...args),
}));

import App from "./App.svelte";

// Every tab's close button shares the uniform "Close tab" label, so a test that
// closes a specific view finds the button via its tab's icon ligature ("code"
// editor, "music_note" render, "preview" preview). Throws if no such tab is
// open, so a stale target fails loudly.
function closeTabBtn(container: ParentNode, icon: string): HTMLElement {
  const wrap = [...container.querySelectorAll(".tab-wrap")].find(
    (w) => w.querySelector(".tab-icon")?.textContent === icon,
  );
  if (!wrap) throw new Error(`no open tab with icon "${icon}"`);
  return wrap.querySelector<HTMLElement>(".tab-close")!;
}
const closeRender = (c: ParentNode) => closeTabBtn(c, "music_note");
const closeEditor = (c: ParentNode) => closeTabBtn(c, "code");

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

  // Zoom is read off the rendered svg's --tab-zoom var; zoom lives on Cmd/Ctrl
  // +/- and the tab-strip Fit control.
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
    const { container } = render(App);

    await fireEvent.click(screen.getByLabelText("Open"));

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
    const { container } = render(App);

    await fireEvent.click(screen.getByLabelText("Open"));

    // The entry becomes the open document...
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      );
    });
    // ...and the project's files (entry + sibling lib) flow to compile as the
    // import map, so an `import` resolves against them.
    await vi.waitFor(() => {
      const maps = wasmCompileMock.mock.calls.map((c) => c[2]);
      expect(maps).toContainEqual(
        expect.objectContaining({ "lib.ctab": "def roll() { 3:0 }" }),
      );
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

    await fireEvent.click(screen.getByLabelText("Open"));
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      ),
    );

    // Click the lib in the dock: it opens as its own focused editor tab.
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
    await fireEvent.click(screen.getByLabelText("Open"));
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      ),
    );
    await fireEvent.click(getByText("lib.ctab"));
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "lib.ctab",
      ),
    );

    // Activating the entry's editor tab (the first one — the lib was appended
    // after it) makes it the active document again. Tabs label by filename now
    // (D49); the icon ligature ("code") distinguishes the editor view type.
    const entryTab = [...container.querySelectorAll(".tab")].find(
      (t) =>
        t.querySelector(".tab-icon")?.textContent === "code" &&
        t.querySelector(".tab-title")?.textContent === "tune.ctab",
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
    // Tabs label by filename now (D49); count editor tabs by the "code" icon.
    const editorTabs = () =>
      [...container.querySelectorAll(".tab-icon")].filter(
        (t) => t.textContent === "code",
      );

    // Open the bundle, then a dock lib — two editor tabs open in this project.
    await fireEvent.click(screen.getByLabelText("Open"));
    await vi.waitFor(() =>
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      ),
    );
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
    await fireEvent.click(screen.getByLabelText("Open"));
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
    render(App);

    await fireEvent.click(screen.getByLabelText("Open"));
    await vi.waitFor(() => expect(openProjectMock).toHaveBeenCalled());

    await fireEvent.click(screen.getByLabelText("Save"));
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
    const { container } = render(App);

    await fireEvent.click(screen.getByLabelText("Save"));

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

    // Opening a project replaces the current one; since the current doc is
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
    const editorTabs = [...container.querySelectorAll(".tab-icon")].filter(
      (t) => t.textContent === "code",
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

  it("closes one view at a time, the session outliving its views", async () => {
    const { container } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    // Closing the render (a clean doc → no prompt) drops only that view; the
    // editor and the document's session remain.
    await fireEvent.click(closeRender(container));
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).toBeNull();
      expect(container.querySelector(".cm-content")).not.toBeNull();
    });
    expect(container.querySelector(".dialog")).toBeNull();
    expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
      "untitled",
    );

    // Closing the editor too removes the last view, emptying the layout.
    await fireEvent.click(closeEditor(container));
    await vi.waitFor(() =>
      expect(container.querySelector(".cm-content")).toBeNull(),
    );
    expect(container.querySelectorAll(".tab")).toHaveLength(0);
  });

  it("closes the focused tab on Cmd/Ctrl-W", async () => {
    const { container } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    // Focus the render, then Cmd/Ctrl-W closes it (clean doc → no prompt),
    // leaving the editor open.
    await fireEvent.pointerDown(container.querySelector(".render-side")!);
    await fireEvent.keyDown(window, { key: "w", metaKey: true });
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).toBeNull();
      expect(container.querySelector(".cm-content")).not.toBeNull();
    });

    // Focus the editor and close it too — the layout empties.
    await fireEvent.pointerDown(container.querySelector(".editor-pane")!);
    await fireEvent.keyDown(window, { key: "w", ctrlKey: true });
    await vi.waitFor(() =>
      expect(container.querySelectorAll(".tab")).toHaveLength(0),
    );
  });

  it("reopens a closed render from the active group's render control", async () => {
    const { container, getByLabelText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    // Close the render — it's gone, and the editor group's control set (now
    // active) invites reopening it.
    await fireEvent.click(closeRender(container));
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

  it("closing the editor leaves its render view open", async () => {
    const { container } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    // Views are independent: closing the (clean) editor keeps the render showing.
    await fireEvent.click(closeEditor(container));
    await vi.waitFor(() =>
      expect(container.querySelector(".cm-content")).toBeNull(),
    );
    expect(container.querySelector("svg.tab")).not.toBeNull();
    expect(container.querySelector(".dialog")).toBeNull();
  });

  it("guards the editor close of a dirty doc, then the final-view discard", async () => {
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

    // Closing the editor of a dirty doc warns; cancelling keeps it editable.
    await fireEvent.click(closeEditor(container));
    let cancelBtn!: HTMLElement;
    await vi.waitFor(() => {
      cancelBtn = container.querySelector(".dialog .cancel")!;
      expect(cancelBtn).toBeTruthy();
    });
    await fireEvent.click(cancelBtn);
    expect(container.querySelector(".cm-content")).not.toBeNull();

    // Confirming closes the editor; the render keeps the dirty doc on screen.
    await fireEvent.click(closeEditor(container));
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
    await fireEvent.click(closeRender(container));
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

  it("reseeds an editor|render layout when New runs on an emptied workspace", async () => {
    const { container, getByLabelText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    // Empty the layout: close the render, then the (clean) editor.
    await fireEvent.click(closeRender(container));
    await fireEvent.click(closeEditor(container));
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

  it("exports the project as a .ctabz bundle from the Export menu", async () => {
    saveBundleMock.mockReset();
    saveBundleMock.mockResolvedValue({
      path: "/proj.ctabz",
      name: "proj.ctabz",
    });
    const { getByText } = render(App);

    // The bundle is an export now (alongside SVG/PNG), not a separate "Save".
    await fireEvent.click(screen.getByLabelText("Export"));
    await fireEvent.click(getByText("Export Bundle (.ctabz)"));

    await vi.waitFor(() => expect(saveBundleMock).toHaveBeenCalled());
    // It carries the entry name plus the live editor source under it, and always
    // prompts for a destination (path: null — a derived artifact).
    const [bundle, target] = saveBundleMock.mock.calls[0] as [
      { entry: string; files: Record<string, string> },
      { path: string | null; suggestedName: string },
    ];
    expect(bundle.entry).toBe("untitled.ctab");
    expect(bundle.files["untitled.ctab"]).toContain("Cripple Creek");
    expect(target.path).toBeNull();
  });

  it("exports the rendered tab as a standalone SVG", async () => {
    saveSvgMock.mockReset();
    saveSvgMock.mockResolvedValue({ path: null, name: "untitled.svg" });
    const { container, getByText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    // Export lives behind the topbar download menu now.
    await fireEvent.click(screen.getByLabelText("Export"));
    await fireEvent.click(getByText("Export SVG"));

    await vi.waitFor(() => expect(saveSvgMock).toHaveBeenCalled());
    const [svg, name] = saveSvgMock.mock.calls[0] as [string, string];
    // The real serializer ran on the current render tree; the io seam swaps the
    // .ctab base name for .svg.
    expect(svg).toContain("<svg");
    expect(name).toBe("untitled.ctab");
    // The bottom bar flashes the export-success notice.
    await vi.waitFor(() =>
      expect(screen.getByText("Exported untitled.svg")).toBeTruthy(),
    );
  });

  it("exports the rendered tab as a PNG via the rasterizer", async () => {
    savePngMock.mockReset();
    svgToPngBlobMock.mockClear();
    savePngMock.mockResolvedValue({ path: null, name: "untitled.png" });
    const { container, getByText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    await fireEvent.click(screen.getByLabelText("Export"));
    await fireEvent.click(getByText("Export PNG"));

    await vi.waitFor(() => expect(savePngMock).toHaveBeenCalled());
    // The SVG was rasterized to a blob before saving.
    expect(svgToPngBlobMock).toHaveBeenCalled();
    const [blob, name] = savePngMock.mock.calls[0] as [Blob, string];
    expect(blob.type).toBe("image/png");
    expect(name).toBe("untitled.ctab");
  });

  it("exports the document as a paginated PDF", async () => {
    savePdfMock.mockReset();
    paginatedTreeToPdfMock.mockClear();
    wasmPaginateMock.mockClear();
    savePdfMock.mockResolvedValue({ path: null, name: "untitled.pdf" });
    const { container, getByText } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    await fireEvent.click(screen.getByLabelText("Export"));
    await fireEvent.click(getByText("Export PDF"));

    await vi.waitFor(() => expect(savePdfMock).toHaveBeenCalled());
    // The document was paginated, then painted to PDF bytes before saving.
    expect(wasmPaginateMock).toHaveBeenCalled();
    expect(paginatedTreeToPdfMock).toHaveBeenCalled();
    const [bytes, name] = savePdfMock.mock.calls[0] as [Uint8Array, string];
    expect(bytes).toBeInstanceOf(Uint8Array);
    expect(name).toBe("untitled.ctab");
    // The bottom bar flashes the export-success notice (the only feedback now
    // that exports skip the save dialog).
    await vi.waitFor(() =>
      expect(screen.getByText("Exported untitled.pdf")).toBeTruthy(),
    );
  });

  it("opens and dismisses the topbar Export menu", async () => {
    render(App);
    // Closed by default; the icon opens an SVG/PNG menu.
    expect(screen.queryByText("Export SVG")).toBeNull();
    await fireEvent.click(screen.getByLabelText("Export"));
    expect(screen.getByText("Export SVG")).toBeTruthy();
    expect(screen.getByText("Export PNG")).toBeTruthy();

    // Escape closes it...
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByText("Export SVG")).toBeNull();

    // ...as does a pointer down outside the menu.
    await fireEvent.click(screen.getByLabelText("Export"));
    await fireEvent.pointerDown(document.body);
    expect(screen.queryByText("Export SVG")).toBeNull();
  });

  it("opens a new document from the New + menu as its own untitled tab", async () => {
    const { container, getAllByLabelText } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector(".cm-content")).toBeTruthy();
    });

    // The tab-strip New "+" (on the active group) opens a template menu; picking
    // one opens a fresh untitled tab and focuses it. A New draft is never-saved,
    // so it's dirty from birth (the "•").
    await fireEvent.click(getAllByLabelText("New tab")[0]);
    await fireEvent.click(screen.getByText("Guitar (standard)"));
    await vi.waitFor(() => {
      expect(container.querySelector(".cm-content")?.textContent).toContain(
        "instrument guitar",
      );
    });
    expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
      "untitled •",
    );
    // The default document is still open alongside it (two editor tabs), with no
    // discard prompt — New never replaces the current doc.
    const editorTabs = [...container.querySelectorAll(".tab-icon")].filter(
      (t) => t.textContent === "code",
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
    const { container } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    await fireEvent.click(screen.getByLabelText("Preview"));

    // A Preview tab appears and renders the export SVG on a white sheet. Tabs
    // label by filename now (D49); the "preview" icon marks the preview view.
    await vi.waitFor(() => {
      const icons = [...container.querySelectorAll(".tab-icon")].map(
        (t) => t.textContent,
      );
      expect(icons).toContain("preview");
      expect(container.querySelector(".sheet svg")).not.toBeNull();
    });
    // The preview is the standalone export SVG, not the live themed render.
    expect(
      container.querySelector('.sheet svg rect[fill="#ffffff"]'),
    ).not.toBeNull();
  });

  it("opens the help view as a singleton tab from the bottom bar", async () => {
    const { container } = render(App);
    await vi.waitFor(() =>
      expect(container.querySelector("svg.tab")).not.toBeNull(),
    );

    await fireEvent.click(screen.getByLabelText("Open help"));

    // A Help tab appears (the "help" icon) and the getting-started content shows.
    await vi.waitFor(() => {
      const icons = [...container.querySelectorAll(".tab-icon")].map(
        (t) => t.textContent,
      );
      expect(icons).toContain("help");
      expect(container.querySelector(".help h1")?.textContent).toContain(
        "cadtab",
      );
    });

    // Re-opening is idempotent: still one Help tab (the singleton is reused).
    await fireEvent.click(screen.getByLabelText("Open help"));
    const helpTabs = [...container.querySelectorAll(".tab-title")].filter(
      (t) => t.textContent === "Help",
    );
    expect(helpTabs).toHaveLength(1);
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
    // The dock is open by default, then hidden on the shortcut.
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    expect(container.querySelector(".dock")).not.toBeNull();

    await fireEvent.keyDown(window, { key: "b", metaKey: true });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(container.querySelector(".dock")).toBeNull();

    await fireEvent.keyDown(window, { key: "b", ctrlKey: true });
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    expect(container.querySelector(".dock")).not.toBeNull();
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
    const { container } = render(App);

    await fireEvent.click(screen.getByLabelText("Open"));
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "tune.ctab",
      );
    });

    // The dock shows the entry (active) and the sibling lib, headed by the
    // bundle name.
    expect(container.querySelector(".dock-title")?.textContent).toBe(
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

  it("surfaces drafts in the dock: a clean starter, then a dirty New draft", async () => {
    const { container, getAllByLabelText } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector(".cm-content")).toBeTruthy();
    });

    // The starter is an unsaved draft but kept clean, so the dock lists it with
    // no unsaved dot.
    expect(
      [...container.querySelectorAll(".dock .file-name")].map(
        (n) => n.textContent,
      ),
    ).toEqual(["untitled"]);
    expect(container.querySelectorAll(".dock .dot")).toHaveLength(0);

    // A New draft joins the dock and is dirty from birth (its row + the topbar
    // both carry the unsaved dot).
    await fireEvent.click(getAllByLabelText("New tab")[0]);
    await fireEvent.click(screen.getByText("Guitar (standard)"));
    await vi.waitFor(() => {
      expect(container.querySelectorAll(".dock .file-name")).toHaveLength(2);
    });
    expect(container.querySelectorAll(".dock .dot")).toHaveLength(1);
    expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
      "untitled •",
    );

    // Clicking the clean starter row in the dock refocuses it — the topbar's
    // active doc (and its dot) follow.
    await fireEvent.click(container.querySelector(".dock .file")!);
    await vi.waitFor(() => {
      expect(container.querySelector(".doc-name")?.textContent?.trim()).toBe(
        "untitled",
      );
    });
  });

  it("desktop: hides the topbar Open and opens a folder from the dock into the tree", async () => {
    setDesktop(true);
    try {
      openFolderMock.mockReset();
      openFolderMock.mockResolvedValue({
        root: "/proj",
        name: "proj",
        files: { "tune.ctab": "score {}", "licks/roll.ctab": "def roll() {}" },
        filePaths: {
          "tune.ctab": "/proj/tune.ctab",
          "licks/roll.ctab": "/proj/licks/roll.ctab",
        },
        dirs: ["licks"],
      });
      const { container } = render(App);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });

      // The topbar Open button is web-only; on desktop you open via the dock /
      // Cmd+O.
      expect(screen.queryByLabelText("Open")).toBeNull();

      // Open a folder from the dock's header control.
      await fireEvent.click(screen.getByLabelText("Open Folder"));

      // The dock shows the real folder tree, headed by the folder name...
      await vi.waitFor(() => {
        expect(container.querySelector(".dock-title")?.textContent).toBe(
          "proj",
        );
      });
      expect(
        [...container.querySelectorAll(".dock .file-name")].map(
          (n) => n.textContent,
        ),
      ).toEqual(["licks", "roll.ctab", "tune.ctab"]);

      // ...and no file is open — the workspace rests on its empty placeholder.
      expect(container.querySelector(".cm-content")).toBeNull();
      expect(screen.getAllByLabelText("New tab").length).toBeGreaterThan(0);
    } finally {
      setDesktop(false);
    }
  });

  it("desktop: renders an empty folder from the scan's directory keys", async () => {
    setDesktop(true);
    try {
      openFolderMock.mockReset();
      openFolderMock.mockResolvedValue({
        root: "/proj",
        name: "proj",
        files: { "tune.ctab": "score {}" },
        filePaths: { "tune.ctab": "/proj/tune.ctab" },
        dirs: ["drafts"],
      });
      const { container } = render(App);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });

      await fireEvent.click(screen.getByLabelText("Open Folder"));

      // The empty `drafts` folder (no .ctab files) still renders, sorted before
      // the root file.
      await vi.waitFor(() => {
        expect(
          [...container.querySelectorAll(".dock .file-name")].map(
            (n) => n.textContent,
          ),
        ).toEqual(["drafts", "tune.ctab"]);
      });
      expect(container.querySelector(".dock .folder")?.textContent).toContain(
        "drafts",
      );
    } finally {
      setDesktop(false);
    }
  });

  it("desktop: New File from the dock creates the file and opens it as a tab", async () => {
    setDesktop(true);
    try {
      openFolderMock.mockReset();
      createFileMock.mockReset();
      createFileMock.mockResolvedValue(undefined);
      openFolderMock.mockResolvedValue({
        root: "/proj",
        name: "proj",
        files: { "tune.ctab": "score {}" },
        filePaths: { "tune.ctab": "/proj/tune.ctab" },
        dirs: [],
      });
      const { container } = render(App);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });

      await fireEvent.click(screen.getByLabelText("Open Folder"));
      await vi.waitFor(() => {
        expect(container.querySelector(".dock-title")?.textContent).toBe(
          "proj",
        );
      });

      // Right-click empty dock space → New File → type a name → Enter.
      await fireEvent.contextMenu(screen.getByLabelText("Project files"));
      await fireEvent.click(screen.getByText("New File"));
      const input = screen.getByLabelText("Name") as HTMLInputElement;
      await fireEvent.input(input, { target: { value: "newtune" } });
      await fireEvent.keyDown(input, { key: "Enter" });

      // The .ctab file is created at the live folder, then opened as an editor tab
      // and listed in the dock.
      await vi.waitFor(() =>
        expect(createFileMock).toHaveBeenCalledWith("/proj/newtune.ctab", ""),
      );
      await vi.waitFor(() => {
        expect(
          [...container.querySelectorAll(".dock .file-name")].map(
            (n) => n.textContent,
          ),
        ).toContain("newtune.ctab");
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });
    } finally {
      setDesktop(false);
    }
  });

  it("desktop: New Folder creates the directory and renders it empty in the dock", async () => {
    setDesktop(true);
    try {
      openFolderMock.mockReset();
      createDirMock.mockReset();
      createDirMock.mockResolvedValue(undefined);
      openFolderMock.mockResolvedValue({
        root: "/proj",
        name: "proj",
        files: { "tune.ctab": "score {}" },
        filePaths: { "tune.ctab": "/proj/tune.ctab" },
        dirs: [],
      });
      const { container } = render(App);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });

      await fireEvent.click(screen.getByLabelText("Open Folder"));
      await vi.waitFor(() => {
        expect(container.querySelector(".dock-title")?.textContent).toBe(
          "proj",
        );
      });

      await fireEvent.contextMenu(screen.getByLabelText("Project files"));
      await fireEvent.click(screen.getByText("New Folder"));
      const input = screen.getByLabelText("Name") as HTMLInputElement;
      await fireEvent.input(input, { target: { value: "drafts" } });
      await fireEvent.keyDown(input, { key: "Enter" });

      await vi.waitFor(() =>
        expect(createDirMock).toHaveBeenCalledWith("/proj/drafts"),
      );
      await vi.waitFor(() => {
        expect(container.querySelector(".dock .folder")?.textContent).toContain(
          "drafts",
        );
      });
    } finally {
      setDesktop(false);
    }
  });

  it("desktop: Delete removes the file, closes its tab and drops the dock row", async () => {
    setDesktop(true);
    try {
      openFolderMock.mockReset();
      removePathMock.mockReset();
      removePathMock.mockResolvedValue(undefined);
      openFolderMock.mockResolvedValue({
        root: "/proj",
        name: "proj",
        files: { "tune.ctab": "score { 3:0 }" },
        filePaths: { "tune.ctab": "/proj/tune.ctab" },
        dirs: [],
      });
      const { container } = render(App);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });

      await fireEvent.click(screen.getByLabelText("Open Folder"));
      await vi.waitFor(() => {
        expect(container.querySelector(".dock .file")).toBeTruthy();
      });
      // Open the file so it has a live tab.
      await fireEvent.click(container.querySelector(".dock .file")!);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")?.textContent).toContain(
          "3:0",
        );
      });

      // Right-click the dock file row → Delete → confirm in the modal.
      await fireEvent.contextMenu(container.querySelector(".dock .file")!);
      await fireEvent.click(screen.getByText("Delete"));
      await fireEvent.click(container.querySelector(".btn.confirm")!);

      await vi.waitFor(() =>
        expect(removePathMock).toHaveBeenCalledWith("/proj/tune.ctab", false),
      );
      // The tab closed (no editor) and the dock row is gone.
      await vi.waitFor(() => {
        expect(container.querySelector(".dock .file")).toBeNull();
        expect(container.querySelector(".cm-content")).toBeNull();
      });
    } finally {
      setDesktop(false);
    }
  });

  it("desktop: Rename moves the file and its open tab follows", async () => {
    setDesktop(true);
    try {
      openFolderMock.mockReset();
      renamePathMock.mockReset();
      renamePathMock.mockResolvedValue(undefined);
      openFolderMock.mockResolvedValue({
        root: "/proj",
        name: "proj",
        files: { "tune.ctab": "score { 3:0 }" },
        filePaths: { "tune.ctab": "/proj/tune.ctab" },
        dirs: [],
      });
      const { container } = render(App);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });

      await fireEvent.click(screen.getByLabelText("Open Folder"));
      await vi.waitFor(() => {
        expect(container.querySelector(".dock .file")).toBeTruthy();
      });
      await fireEvent.click(container.querySelector(".dock .file")!);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")?.textContent).toContain(
          "3:0",
        );
      });

      // Right-click the file → Rename → the inline input is seeded with the name.
      await fireEvent.contextMenu(container.querySelector(".dock .file")!);
      await fireEvent.click(screen.getByText("Rename"));
      const input = screen.getByLabelText("Name") as HTMLInputElement;
      expect(input.value).toBe("tune.ctab");
      await fireEvent.input(input, { target: { value: "renamed" } });
      await fireEvent.keyDown(input, { key: "Enter" });

      // The file is moved on disk, the dock row relabels, and the open tab follows
      // (still showing the same buffer).
      await vi.waitFor(() =>
        expect(renamePathMock).toHaveBeenCalledWith(
          "/proj/tune.ctab",
          "/proj/renamed.ctab",
        ),
      );
      await vi.waitFor(() => {
        expect(
          [...container.querySelectorAll(".dock .file-name")].map(
            (n) => n.textContent,
          ),
        ).toEqual(["renamed.ctab"]);
        expect(container.querySelector(".cm-content")?.textContent).toContain(
          "3:0",
        );
      });
    } finally {
      setDesktop(false);
    }
  });

  it("desktop: saves an opened folder file back to its real fs path", async () => {
    setDesktop(true);
    try {
      openFolderMock.mockReset();
      saveDocumentMock.mockReset();
      saveDocumentMock.mockResolvedValue({
        path: "/proj/tune.ctab",
        name: "tune.ctab",
      });
      openFolderMock.mockResolvedValue({
        root: "/proj",
        name: "proj",
        files: { "tune.ctab": "score {}" },
        filePaths: { "tune.ctab": "/proj/tune.ctab" },
        dirs: [],
      });
      const { container } = render(App);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });

      // Open the folder, then open its file from the dock.
      await fireEvent.click(screen.getByLabelText("Open Folder"));
      await vi.waitFor(() => {
        expect(container.querySelector(".dock .file")).toBeTruthy();
      });
      await fireEvent.click(container.querySelector(".dock .file")!);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });

      // Save writes straight back to the file's real path — no dialog.
      await fireEvent.click(screen.getByLabelText("Save"));
      await vi.waitFor(() => expect(saveDocumentMock).toHaveBeenCalled());
      const [, target] = saveDocumentMock.mock.calls[0] as [
        string,
        { path: string | null; suggestedName: string },
      ];
      expect(target.path).toBe("/proj/tune.ctab");
    } finally {
      setDesktop(false);
    }
  });

  it("desktop: live-reloads an open file when it changes on disk", async () => {
    setDesktop(true);
    try {
      openFolderMock.mockReset();
      rescanFolderMock.mockReset();
      watchCallback = null;
      openFolderMock.mockResolvedValue({
        root: "/proj",
        name: "proj",
        files: { "tune.ctab": "score { 3:0 }" },
        filePaths: { "tune.ctab": "/proj/tune.ctab" },
        dirs: [],
      });
      const { container } = render(App);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });

      // Open the folder, then its file, so it's live in an editor tab.
      await fireEvent.click(screen.getByLabelText("Open Folder"));
      await vi.waitFor(() => {
        expect(container.querySelector(".dock .file")).toBeTruthy();
      });
      await fireEvent.click(container.querySelector(".dock .file")!);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")?.textContent).toContain(
          "3:0",
        );
      });

      // The folder is watched; a change landing on disk re-scans and the open
      // tab live-reloads to the disk content.
      expect(watchCallback).toBeTypeOf("function");
      rescanFolderMock.mockResolvedValue({
        files: { "tune.ctab": "score { 5:7 }" },
        filePaths: { "tune.ctab": "/proj/tune.ctab" },
        dirs: [],
      });
      watchCallback!();

      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")?.textContent).toContain(
          "5:7",
        );
      });
    } finally {
      setDesktop(false);
    }
  });

  it("desktop: strikes an open file's tab when it's deleted on disk, and saves it back", async () => {
    setDesktop(true);
    try {
      openFolderMock.mockReset();
      rescanFolderMock.mockReset();
      saveDocumentMock.mockReset();
      saveDocumentMock.mockResolvedValue({
        path: "/proj/tune.ctab",
        name: "tune.ctab",
      });
      watchCallback = null;
      openFolderMock.mockResolvedValue({
        root: "/proj",
        name: "proj",
        files: { "tune.ctab": "score { 3:0 }" },
        filePaths: { "tune.ctab": "/proj/tune.ctab" },
        dirs: [],
      });
      const { container } = render(App);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")).toBeTruthy();
      });

      await fireEvent.click(screen.getByLabelText("Open Folder"));
      await vi.waitFor(() => {
        expect(container.querySelector(".dock .file")).toBeTruthy();
      });
      await fireEvent.click(container.querySelector(".dock .file")!);
      await vi.waitFor(() => {
        expect(container.querySelector(".cm-content")?.textContent).toContain(
          "3:0",
        );
      });
      expect(container.querySelector(".tab-title.missing")).toBeNull();

      // Delete it on disk: the re-scan no longer lists it.
      expect(watchCallback).toBeTypeOf("function");
      rescanFolderMock.mockResolvedValue({ files: {}, filePaths: {} });
      watchCallback!();

      // The tab strikes through and the dock row drops, but the buffer stays.
      await vi.waitFor(() => {
        expect(container.querySelector(".tab-title.missing")).not.toBeNull();
      });
      expect(container.querySelector(".dock .file")).toBeNull();
      expect(container.querySelector(".cm-content")?.textContent).toContain(
        "3:0",
      );

      // Saving rewrites it to its original path and clears the strike.
      await fireEvent.click(screen.getByLabelText("Save"));
      await vi.waitFor(() => expect(saveDocumentMock).toHaveBeenCalled());
      const [, target] = saveDocumentMock.mock.calls[0] as [
        string,
        { path: string | null; suggestedName: string },
      ];
      expect(target.path).toBe("/proj/tune.ctab");
      await vi.waitFor(() => {
        expect(container.querySelector(".tab-title.missing")).toBeNull();
      });
    } finally {
      setDesktop(false);
    }
  });

  it("cycles the colour theme onto the document root", async () => {
    const { container } = render(App);
    const toggle = container.querySelector(".theme-toggle")!;
    const root = document.documentElement;

    expect(root.getAttribute("data-theme")).toBe("dark"); // dark by default
    await fireEvent.click(toggle);
    expect(root.getAttribute("data-theme")).toBeNull(); // system
    await fireEvent.click(toggle);
    expect(root.getAttribute("data-theme")).toBe("light");
    await fireEvent.click(toggle);
    expect(root.getAttribute("data-theme")).toBe("dark"); // back to dark
  });

  it("toggles the editor autocomplete setting from the topbar", async () => {
    const { container } = render(App);
    const toggle = container.querySelector<HTMLButtonElement>(
      ".autocomplete-toggle",
    )!;

    // On by default; the button reads its state for assistive tech.
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    expect(toggle.getAttribute("aria-label")).toBe("Autocomplete: on");

    await fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(toggle.getAttribute("aria-label")).toBe("Autocomplete: off");

    await fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
  });

  it("auto-formats on save only when format-on-save is toggled on", async () => {
    const { container } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector(".cm-content")).not.toBeNull();
    });

    // Off by default: saving does not run the formatter.
    wasmFormatMock.mockClear();
    await fireEvent.click(screen.getByLabelText("Save"));
    expect(wasmFormatMock).not.toHaveBeenCalled();

    // Toggle format-on-save on, then save: the doc is canonicalized first and
    // the formatted text replaces the editor buffer.
    await fireEvent.click(container.querySelector(".format-toggle")!);
    await fireEvent.click(screen.getByLabelText("Save"));
    await vi.waitFor(() => {
      expect(wasmFormatMock).toHaveBeenCalled();
    });
    await vi.waitFor(() => {
      const text = container.querySelector(".cm-content")?.textContent ?? "";
      expect(text).toContain('title "Formatted"');
    });
  });
});
