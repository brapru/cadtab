import { describe, it, expect, vi } from "vitest";
import { createLiveCompiler } from "./live";
import type { CompileResult } from "./types";

function resultWithWidth(width: number): CompileResult {
  return {
    renderTree: { meta: { width, height: 1 }, header: [], systems: [] },
    diagnostics: [],
    tokens: [],
  };
}

describe("createLiveCompiler", () => {
  it("drops a stale result that resolves after a newer run", async () => {
    let resolveA!: (r: CompileResult) => void;
    let resolveB!: (r: CompileResult) => void;
    const pending = [
      new Promise<CompileResult>((r) => (resolveA = r)),
      new Promise<CompileResult>((r) => (resolveB = r)),
    ];
    const compileFn = vi.fn(() => pending.shift()!);

    const applied: CompileResult[] = [];
    const live = createLiveCompiler(compileFn, (r) => applied.push(r));

    const runA = live.run("a", { width: 0 });
    const runB = live.run("b", { width: 0 });

    // B (newer) resolves first, then A (older/stale) resolves.
    resolveB(resultWithWidth(2));
    expect(await runB).toBe(true);
    resolveA(resultWithWidth(1));
    expect(await runA).toBe(false);

    expect(applied).toHaveLength(1);
    expect(applied[0].renderTree.meta.width).toBe(2);
  });

  it("applies the result when it is the latest run", async () => {
    const compileFn = vi.fn(async () => resultWithWidth(7));
    const applied: CompileResult[] = [];
    const live = createLiveCompiler(compileFn, (r) => applied.push(r));

    expect(await live.run("x", { width: 0 })).toBe(true);
    expect(applied).toEqual([resultWithWidth(7)]);
  });

  it("routes a latest-run rejection to onError instead of onResult", async () => {
    const boom = new Error("no backend");
    const compileFn = vi.fn(async () => {
      throw boom;
    });
    const applied: CompileResult[] = [];
    const errors: unknown[] = [];
    const live = createLiveCompiler(
      compileFn,
      (r) => applied.push(r),
      (e) => errors.push(e),
    );

    expect(await live.run("x", { width: 0 })).toBe(true);
    expect(applied).toEqual([]);
    expect(errors).toEqual([boom]);
  });

  it("drops a stale rejection so it cannot clobber a fresh result", async () => {
    let rejectA!: (e: unknown) => void;
    let resolveB!: (r: CompileResult) => void;
    const pending = [
      new Promise<CompileResult>((_, reject) => (rejectA = reject)),
      new Promise<CompileResult>((r) => (resolveB = r)),
    ];
    const compileFn = vi.fn(() => pending.shift()!);

    const applied: CompileResult[] = [];
    const errors: unknown[] = [];
    const live = createLiveCompiler(
      compileFn,
      (r) => applied.push(r),
      (e) => errors.push(e),
    );

    const runA = live.run("a", { width: 0 });
    const runB = live.run("b", { width: 0 });

    // B (newer) resolves first; then A (older/stale) rejects.
    resolveB(resultWithWidth(2));
    expect(await runB).toBe(true);
    rejectA(new Error("stale"));
    expect(await runA).toBe(false);

    expect(applied).toHaveLength(1);
    expect(applied[0].renderTree.meta.width).toBe(2);
    expect(errors).toEqual([]);
  });
});
