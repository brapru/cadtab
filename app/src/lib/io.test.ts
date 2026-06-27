import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const openMock = vi.fn();
const saveMock = vi.fn();
const readTextFileMock = vi.fn();
const writeTextFileMock = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openMock(...args),
  save: (...args: unknown[]) => saveMock(...args),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({
  readTextFile: (...args: unknown[]) => readTextFileMock(...args),
  writeTextFile: (...args: unknown[]) => writeTextFileMock(...args),
}));

import {
  basename,
  withCtabExtension,
  defaultDocName,
  openDocument,
  saveDocument,
} from "./io";

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

  it("opens via the dialog then reads the picked path", async () => {
    openMock.mockResolvedValue("/Users/x/foo.ctab");
    readTextFileMock.mockResolvedValue("CONTENT");

    const result = await openDocument();

    expect(result).toEqual({
      path: "/Users/x/foo.ctab",
      name: "foo.ctab",
      content: "CONTENT",
    });
    expect(readTextFileMock).toHaveBeenCalledWith("/Users/x/foo.ctab");
  });

  it("returns null when the open dialog is cancelled", async () => {
    openMock.mockResolvedValue(null);
    expect(await openDocument()).toBeNull();
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

    expect(await openDocument()).toEqual({
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
    expect(await openDocument()).toBeNull();
  });

  it("resolves null when the picker returns no file", async () => {
    const input = fakeInput({ files: [] });
    input.click = () => input.onchange?.();
    vi.spyOn(document, "createElement").mockReturnValue(
      input as unknown as HTMLElement,
    );
    expect(await openDocument()).toBeNull();
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
});
