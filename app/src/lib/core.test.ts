import { describe, it, expect, vi, beforeEach } from "vitest";
import type { CompileResult } from "./types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const wasmCompileMock = vi.fn();
vi.mock("./wasm", () => ({
  compile: (...args: unknown[]) => wasmCompileMock(...args),
}));

import { compile, isTauri } from "./core";

const fake: CompileResult = {
  renderTree: { meta: { width: 1, height: 1 }, header: [], systems: [] },
  diagnostics: [],
  tokens: [],
};

function setTauri(present: boolean) {
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  if (present) {
    w.__TAURI_INTERNALS__ = {};
  } else {
    delete w.__TAURI_INTERNALS__;
  }
}

describe("core backend dispatch", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    wasmCompileMock.mockReset();
    setTauri(false);
  });

  it("detects Tauri via __TAURI_INTERNALS__", () => {
    expect(isTauri()).toBe(false);
    setTauri(true);
    expect(isTauri()).toBe(true);
  });

  it("dispatches to the Tauri command under Tauri, passing the base path", async () => {
    setTauri(true);
    invokeMock.mockResolvedValue(fake);

    const files = { "rolls.ctab": "def r() { 3:0 }" };
    const result = await compile(
      "3:0",
      { width: 800 },
      { basePath: "/x/a.ctab", files },
    );

    expect(invokeMock).toHaveBeenCalledWith("compile", {
      source: "3:0",
      config: { width: 800 },
      basePath: "/x/a.ctab",
      files,
    });
    expect(wasmCompileMock).not.toHaveBeenCalled();
    expect(result).toBe(fake);
  });

  it("defaults the base path and files when no context is given", async () => {
    setTauri(true);
    invokeMock.mockResolvedValue(fake);

    await compile("3:0", { width: 800 });

    expect(invokeMock).toHaveBeenCalledWith("compile", {
      source: "3:0",
      config: { width: 800 },
      basePath: null,
      files: {},
    });
  });

  it("dispatches to the wasm backend in a plain browser, passing the bundle", async () => {
    wasmCompileMock.mockResolvedValue(fake);
    const files = { "rolls.ctab": "def r() { 3:0 }" };

    const result = await compile("3:0", { width: 800 }, { files });

    expect(wasmCompileMock).toHaveBeenCalledWith("3:0", { width: 800 }, files);
    expect(invokeMock).not.toHaveBeenCalled();
    expect(result).toBe(fake);
  });

  it("defaults the wasm bundle to an empty map when no context is given", async () => {
    wasmCompileMock.mockResolvedValue(fake);

    await compile("3:0", { width: 800 });

    expect(wasmCompileMock).toHaveBeenCalledWith("3:0", { width: 800 }, {});
  });
});
