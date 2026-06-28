import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const openMock = vi.fn();
const saveMock = vi.fn();
const readTextFileMock = vi.fn();
const writeTextFileMock = vi.fn();
const writeFileMock = vi.fn();
const readDirMock = vi.fn();
const watchImmediateMock = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openMock(...args),
  save: (...args: unknown[]) => saveMock(...args),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({
  readTextFile: (...args: unknown[]) => readTextFileMock(...args),
  writeTextFile: (...args: unknown[]) => writeTextFileMock(...args),
  writeFile: (...args: unknown[]) => writeFileMock(...args),
  readDir: (...args: unknown[]) => readDirMock(...args),
  watchImmediate: (...args: unknown[]) => watchImmediateMock(...args),
}));

import {
  basename,
  joinPath,
  toRelative,
  withCtabExtension,
  defaultDocName,
  collectCtabFiles,
  openFolder,
  rescanFolder,
  watchFolder,
  openProject,
  saveDocument,
  saveBundle,
  saveSvg,
  savePng,
  type DirEntry,
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

  it("joinPath uses the directory's own separator", () => {
    expect(joinPath("/Users/x", "tune.ctab")).toBe("/Users/x/tune.ctab");
    expect(joinPath("C:\\scores", "tune.ctab")).toBe("C:\\scores\\tune.ctab");
    expect(joinPath("/Users/x/", "tune.ctab")).toBe("/Users/x/tune.ctab");
    expect(joinPath("", "tune.ctab")).toBe("tune.ctab");
  });

  it("toRelative strips the root and normalizes keys to forward slashes", () => {
    expect(toRelative("/proj", "/proj/licks/roll.ctab")).toBe(
      "licks/roll.ctab",
    );
    expect(toRelative("C:\\proj", "C:\\proj\\licks\\roll.ctab")).toBe(
      "licks/roll.ctab",
    );
    // A path not under root still comes back normalized (leading sep dropped).
    expect(toRelative("/other", "/proj/tune.ctab")).toBe("proj/tune.ctab");
  });
});

describe("collectCtabFiles", () => {
  // A fake tree: dir -> entries, with file contents keyed by absolute path.
  function fakeFs(
    tree: Record<string, DirEntry[]>,
    contents: Record<string, string>,
  ) {
    return {
      readDir: (dir: string) => Promise.resolve(tree[dir] ?? []),
      readFile: (path: string) => Promise.resolve(contents[path] ?? ""),
    };
  }
  const dir = (name: string): DirEntry => ({
    name,
    isDirectory: true,
    isFile: false,
  });
  const file = (name: string): DirEntry => ({
    name,
    isDirectory: false,
    isFile: true,
  });

  it("recurses, keeping .ctab files keyed relative to root with their abs path", async () => {
    const { readDir, readFile } = fakeFs(
      {
        "/proj": [dir("licks"), file("tune.ctab"), file("notes.txt")],
        "/proj/licks": [file("roll.ctab"), file("pinch.ctab")],
      },
      {
        "/proj/tune.ctab": "score {}",
        "/proj/licks/roll.ctab": "def roll() {}",
        "/proj/licks/pinch.ctab": "def pinch() {}",
      },
    );
    const { files, filePaths, dirs } = await collectCtabFiles(
      "/proj",
      readDir,
      readFile,
    );
    expect(files).toEqual({
      "tune.ctab": "score {}",
      "licks/roll.ctab": "def roll() {}",
      "licks/pinch.ctab": "def pinch() {}",
    });
    // Non-.ctab files are skipped; abs paths map back for write-back.
    expect(filePaths["licks/roll.ctab"]).toBe("/proj/licks/roll.ctab");
    // Every directory is reported (relative keys) so the dock can render them.
    expect(dirs).toEqual(["licks"]);
  });

  it("reports empty directories so they still render (and excludes dot-dirs)", async () => {
    const { readDir, readFile } = fakeFs(
      {
        "/proj": [dir("empty"), dir("licks"), dir(".git"), file("tune.ctab")],
        "/proj/empty": [],
        "/proj/licks": [file("roll.ctab")],
        "/proj/.git": [file("config.ctab")],
      },
      {
        "/proj/tune.ctab": "score {}",
        "/proj/licks/roll.ctab": "def roll() {}",
      },
    );
    const { files, dirs } = await collectCtabFiles("/proj", readDir, readFile);
    // Walk order: `empty` (no files), then `licks/roll.ctab`, then the root file.
    expect(Object.keys(files)).toEqual(["licks/roll.ctab", "tune.ctab"]);
    // `empty` holds no .ctab files but is still reported; `.git` is skipped.
    expect(dirs).toEqual(["empty", "licks"]);
  });

  it("skips a listed file that vanishes (errors) on read, keeping the rest", async () => {
    const { readDir } = fakeFs(
      {
        "/proj": [file("gone.ctab"), file("tune.ctab")],
      },
      { "/proj/tune.ctab": "score {}" },
    );
    // gone.ctab is listed but throws on read (deleted mid-scan).
    const readFile = (path: string) =>
      path === "/proj/gone.ctab"
        ? Promise.reject(new Error("ENOENT"))
        : Promise.resolve("score {}");
    const { files } = await collectCtabFiles("/proj", readDir, readFile);
    expect(Object.keys(files)).toEqual(["tune.ctab"]);
  });

  it("skips dot-directories", async () => {
    const { readDir, readFile } = fakeFs(
      {
        "/proj": [dir(".git"), file("tune.ctab")],
        "/proj/.git": [file("config.ctab")],
      },
      { "/proj/tune.ctab": "score {}", "/proj/.git/config.ctab": "x" },
    );
    const { files } = await collectCtabFiles("/proj", readDir, readFile);
    expect(Object.keys(files)).toEqual(["tune.ctab"]);
  });
});

describe("io desktop (Tauri) backend", () => {
  beforeEach(() => {
    openMock.mockReset();
    saveMock.mockReset();
    readTextFileMock.mockReset();
    writeTextFileMock.mockReset();
    readDirMock.mockReset();
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

  it("opens a folder: picks a directory and reads its .ctab tree", async () => {
    openMock.mockResolvedValue("/proj");
    readDirMock.mockImplementation((dir: string) =>
      Promise.resolve(
        dir === "/proj"
          ? [
              { name: "licks", isDirectory: true, isFile: false },
              { name: "tune.ctab", isDirectory: false, isFile: true },
            ]
          : [{ name: "roll.ctab", isDirectory: false, isFile: true }],
      ),
    );
    readTextFileMock.mockImplementation((p: string) =>
      Promise.resolve(p === "/proj/tune.ctab" ? "score {}" : "def roll() {}"),
    );

    const folder = await openFolder();

    expect(openMock).toHaveBeenCalledWith({ directory: true });
    expect(folder).toEqual({
      root: "/proj",
      name: "proj",
      files: { "tune.ctab": "score {}", "licks/roll.ctab": "def roll() {}" },
      filePaths: {
        "tune.ctab": "/proj/tune.ctab",
        "licks/roll.ctab": "/proj/licks/roll.ctab",
      },
      dirs: ["licks"],
    });
  });

  it("returns null when the folder dialog is cancelled", async () => {
    openMock.mockResolvedValue(null);
    expect(await openFolder()).toBeNull();
    expect(readDirMock).not.toHaveBeenCalled();
  });

  it("rescanFolder re-reads the tree without a picker", async () => {
    readDirMock.mockResolvedValue([
      { name: "tune.ctab", isDirectory: false, isFile: true },
    ]);
    readTextFileMock.mockResolvedValue("score { 5:7 }");

    const scan = await rescanFolder("/proj");

    expect(openMock).not.toHaveBeenCalled();
    expect(scan).toEqual({
      files: { "tune.ctab": "score { 5:7 }" },
      filePaths: { "tune.ctab": "/proj/tune.ctab" },
      dirs: [],
    });
  });

  it("watchFolder wires a recursive immediate watch and returns the unwatch", async () => {
    const unwatch = vi.fn();
    watchImmediateMock.mockResolvedValue(unwatch);
    const onChange = vi.fn();

    const stop = await watchFolder("/proj", onChange);

    // Uses watchImmediate (raw notify), not the debounced watch, so file removals
    // reach the callback on macOS.
    const [path, cb, options] = watchImmediateMock.mock.calls[0] as [
      string,
      () => void,
      { recursive: boolean },
    ];
    expect(path).toBe("/proj");
    expect(options).toMatchObject({ recursive: true });
    // The watch callback funnels through to onChange.
    cb();
    expect(onChange).toHaveBeenCalled();
    expect(stop).toBe(unwatch);
  });
});

describe("io web backend", () => {
  beforeEach(() => setTauri(false));
  afterEach(() => vi.restoreAllMocks());

  it("openFolder is a no-op (null) off-desktop", async () => {
    expect(await openFolder()).toBeNull();
  });

  it("watchFolder is a no-op off-desktop, returning a usable unwatch", async () => {
    const stop = await watchFolder("/proj", () => {});
    expect(watchImmediateMock).not.toHaveBeenCalled();
    expect(() => stop()).not.toThrow();
  });

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
