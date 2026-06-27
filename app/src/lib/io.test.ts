import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const openMock = vi.fn();
const saveMock = vi.fn();
const readTextFileMock = vi.fn();
const writeTextFileMock = vi.fn();
const writeFileMock = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openMock(...args),
  save: (...args: unknown[]) => saveMock(...args),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({
  readTextFile: (...args: unknown[]) => readTextFileMock(...args),
  writeTextFile: (...args: unknown[]) => writeTextFileMock(...args),
  writeFile: (...args: unknown[]) => writeFileMock(...args),
}));

import {
  basename,
  withCtabExtension,
  defaultDocName,
  openProject,
  saveDocument,
  saveBundle,
  saveSvg,
  savePng,
} from "./io";
import { serializeBundle, type ProjectBundle } from "./bundle";

function setTauri(present: boolean) {
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  if (present) w.__TAURI_INTERNALS__ = {};
  else delete w.__TAURI_INTERNALS__;
}

describe("io path/name helpers", () => {
  it("basename takes the last path segment, posix or windows", () => {
    expect(basename("/Users/x/tune.ctab")).toBe("tune.ctab");
    expect(basename("C:\\scores\\tune.ctab")).toBe("tune.ctab");
    expect(basename("bare.ctab")).toBe("bare.ctab");
  });

  it("withCtabExtension appends the extension only when missing", () => {
    expect(withCtabExtension("song")).toBe("song.ctab");
    expect(withCtabExtension("song.ctab")).toBe("song.ctab");
    // Case-insensitive match leaves an existing extension untouched.
    expect(withCtabExtension("song.CTAB")).toBe("song.CTAB");
    expect(withCtabExtension("  ")).toBe("untitled.ctab");
  });

  it("defaultDocName slugifies the title declaration, else untitled", () => {
    expect(defaultDocName('title "Cripple Creek"\nscore {}')).toBe(
      "cripple-creek.ctab",
    );
    expect(defaultDocName('title "A/B #2"')).toBe("a-b-2.ctab");
    expect(defaultDocName("score { 3:0 }")).toBe("untitled.ctab");
    expect(defaultDocName('title "   "')).toBe("untitled.ctab");
  });
});

describe("io desktop (Tauri) backend", () => {
  beforeEach(() => {
    openMock.mockReset();
    saveMock.mockReset();
    readTextFileMock.mockReset();
    writeTextFileMock.mockReset();
    setTauri(true);
  });
  afterEach(() => setTauri(false));

  it("opens a single score via the dialog and reads the picked path", async () => {
    openMock.mockResolvedValue("/Users/x/foo.ctab");
    readTextFileMock.mockResolvedValue("CONTENT");

    const result = await openProject();

    expect(result).toEqual({
      kind: "single",
      path: "/Users/x/foo.ctab",
      name: "foo.ctab",
      content: "CONTENT",
    });
    expect(readTextFileMock).toHaveBeenCalledWith("/Users/x/foo.ctab");
  });

  it("opens a `.ctabz` as a parsed project bundle", async () => {
    const bundle: ProjectBundle = {
      entry: "tune.ctab",
      files: { "tune.ctab": "score { 3:0 }", "lib.ctab": "def l() { 3:0 }" },
    };
    openMock.mockResolvedValue("/Users/x/proj.ctabz");
    readTextFileMock.mockResolvedValue(serializeBundle(bundle));

    const result = await openProject();

    expect(result).toEqual({
      kind: "bundle",
      path: "/Users/x/proj.ctabz",
      name: "proj.ctabz",
      bundle,
    });
  });

  it("rejects when a chosen bundle is malformed", async () => {
    openMock.mockResolvedValue("/Users/x/bad.ctabz");
    readTextFileMock.mockResolvedValue("{not json");
    await expect(openProject()).rejects.toThrow(/invalid JSON/);
  });

  it("returns null when the open dialog is cancelled", async () => {
    openMock.mockResolvedValue(null);
    expect(await openProject()).toBeNull();
    expect(readTextFileMock).not.toHaveBeenCalled();
  });

  it("overwrites a known path in place without a dialog", async () => {
    writeTextFileMock.mockResolvedValue(undefined);

    const result = await saveDocument("DATA", {
      path: "/Users/x/bar.ctab",
      suggestedName: "bar.ctab",
    });

    expect(result).toEqual({ path: "/Users/x/bar.ctab", name: "bar.ctab" });
    expect(saveMock).not.toHaveBeenCalled();
    expect(writeTextFileMock).toHaveBeenCalledWith("/Users/x/bar.ctab", "DATA");
  });

  it("prompts a save dialog when there is no known path", async () => {
    saveMock.mockResolvedValue("/Users/x/new.ctab");
    writeTextFileMock.mockResolvedValue(undefined);

    const result = await saveDocument("DATA", {
      path: null,
      suggestedName: "new.ctab",
    });

    expect(result).toEqual({ path: "/Users/x/new.ctab", name: "new.ctab" });
    expect(saveMock).toHaveBeenCalled();
    expect(writeTextFileMock).toHaveBeenCalledWith("/Users/x/new.ctab", "DATA");
  });

  it("returns null when the save dialog is cancelled", async () => {
    saveMock.mockResolvedValue(null);
    expect(
      await saveDocument("DATA", { path: null, suggestedName: "bar.ctab" }),
    ).toBeNull();
    expect(writeTextFileMock).not.toHaveBeenCalled();
  });

  it("saves a project bundle as serialized `.ctabz`, prompting for a path", async () => {
    saveMock.mockResolvedValue("/Users/x/proj.ctabz");
    writeTextFileMock.mockResolvedValue(undefined);
    const bundle: ProjectBundle = {
      entry: "tune.ctab",
      files: { "tune.ctab": "score { 3:0 }" },
    };

    const result = await saveBundle(bundle, {
      path: null,
      // Seeded from the score name; the dialog filter offers `.ctabz`.
      suggestedName: "tune.ctab",
    });

    expect(result).toEqual({ path: "/Users/x/proj.ctabz", name: "proj.ctabz" });
    // The score name is normalized to the bundle extension for the dialog.
    expect(saveMock).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: "tune.ctab" }),
    );
    const [, written] = writeTextFileMock.mock.calls[0] as [string, string];
    expect(JSON.parse(written)).toEqual({
      version: 1,
      entry: "tune.ctab",
      files: { "tune.ctab": "score { 3:0 }" },
    });
  });

  it("exports an SVG through the text writer", async () => {
    writeTextFileMock.mockReset();
    writeTextFileMock.mockResolvedValue(undefined);

    const result = await saveSvg("<svg/>", {
      path: "/x/tab.svg",
      suggestedName: "tab.svg",
    });

    expect(result).toEqual({ path: "/x/tab.svg", name: "tab.svg" });
    expect(writeTextFileMock).toHaveBeenCalledWith("/x/tab.svg", "<svg/>");
  });

  it("exports a PNG through the binary writer", async () => {
    writeFileMock.mockReset();
    writeFileMock.mockResolvedValue(undefined);
    // jsdom's Blob has no arrayBuffer(); stub the bytes the writer reads.
    const blob = {
      type: "image/png",
      arrayBuffer: async () => new Uint8Array([1, 2, 3]).buffer,
    } as unknown as Blob;

    const result = await savePng(blob, {
      path: "/x/tab.png",
      suggestedName: "tab.png",
    });

    expect(result).toEqual({ path: "/x/tab.png", name: "tab.png" });
    const [path, bytes] = writeFileMock.mock.calls[0] as [string, Uint8Array];
    expect(path).toBe("/x/tab.png");
    expect(Array.from(bytes)).toEqual([1, 2, 3]);
  });
});

describe("io web backend", () => {
  beforeEach(() => setTauri(false));
  afterEach(() => vi.restoreAllMocks());

  type FakeInput = {
    type: string;
    accept: string;
    files: unknown;
    onchange?: () => void;
    oncancel?: () => void;
    click: () => void;
  };
  function fakeInput(over: Partial<FakeInput>): FakeInput {
    return {
      type: "",
      accept: "",
      files: undefined,
      click: () => {},
      ...over,
    };
  }

  it("opens via the browser file picker and reads its text", async () => {
    const file = { name: "tune.ctab", text: () => Promise.resolve("DOC") };
    const input = fakeInput({ files: [file] });
    input.click = () => input.onchange?.();
    vi.spyOn(document, "createElement").mockReturnValue(
      input as unknown as HTMLElement,
    );

    expect(await openProject()).toEqual({
      kind: "single",
      path: null,
      name: "tune.ctab",
      content: "DOC",
    });
  });

  it("resolves null when the file picker is dismissed", async () => {
    const input = fakeInput({});
    input.click = () => input.oncancel?.();
    vi.spyOn(document, "createElement").mockReturnValue(
      input as unknown as HTMLElement,
    );
    expect(await openProject()).toBeNull();
  });

  it("resolves null when the picker returns no file", async () => {
    const input = fakeInput({ files: [] });
    input.click = () => input.onchange?.();
    vi.spyOn(document, "createElement").mockReturnValue(
      input as unknown as HTMLElement,
    );
    expect(await openProject()).toBeNull();
  });

  it("saves by downloading a named .ctab blob", async () => {
    const createObjectURL = vi.fn(() => "blob:1");
    const revokeObjectURL = vi.fn();
    vi.stubGlobal("URL", { createObjectURL, revokeObjectURL });
    const anchor = { href: "", download: "", click: vi.fn() };
    vi.spyOn(document, "createElement").mockReturnValue(
      anchor as unknown as HTMLElement,
    );

    const result = await saveDocument("hello", {
      path: null,
      suggestedName: "My Song",
    });

    expect(result).toEqual({ path: null, name: "My Song.ctab" });
    expect(anchor.href).toBe("blob:1");
    expect(anchor.download).toBe("My Song.ctab");
    expect(anchor.click).toHaveBeenCalled();
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:1");
  });

  it("downloads a bundle as `.ctabz`, swapping the score extension", async () => {
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn(() => "blob:2"),
      revokeObjectURL: vi.fn(),
    });
    const anchor = { href: "", download: "", click: vi.fn() };
    vi.spyOn(document, "createElement").mockReturnValue(
      anchor as unknown as HTMLElement,
    );

    const result = await saveBundle(
      { entry: "tune.ctab", files: { "tune.ctab": "score { 3:0 }" } },
      { path: null, suggestedName: "tune.ctab" },
    );

    // `tune.ctab` becomes `tune.ctabz`, not `tune.ctab.ctabz`.
    expect(result).toEqual({ path: null, name: "tune.ctabz" });
    expect(anchor.download).toBe("tune.ctabz");
  });

  it("downloads an SVG export, swapping the score extension", async () => {
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn(() => "blob:svg"),
      revokeObjectURL: vi.fn(),
    });
    const anchor = { href: "", download: "", click: vi.fn() };
    vi.spyOn(document, "createElement").mockReturnValue(
      anchor as unknown as HTMLElement,
    );

    const result = await saveSvg("<svg/>", {
      path: null,
      suggestedName: "tune.ctab",
    });

    expect(result).toEqual({ path: null, name: "tune.svg" });
    expect(anchor.download).toBe("tune.svg");
  });

  it("downloads a PNG export blob with the .png extension", async () => {
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn(() => "blob:png"),
      revokeObjectURL: vi.fn(),
    });
    const anchor = { href: "", download: "", click: vi.fn() };
    vi.spyOn(document, "createElement").mockReturnValue(
      anchor as unknown as HTMLElement,
    );
    const blob = new Blob([new Uint8Array([1])], { type: "image/png" });

    const result = await savePng(blob, {
      path: null,
      suggestedName: "tune.ctab",
    });

    expect(result).toEqual({ path: null, name: "tune.png" });
    expect(anchor.download).toBe("tune.png");
  });
});
