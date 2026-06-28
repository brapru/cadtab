import { describe, it, expect } from "vitest";
import { reconcileScan, type FolderScan } from "./watch";

const scan = (
  files: Record<string, string>,
  dirs: string[] = [],
): FolderScan => ({
  files,
  filePaths: Object.fromEntries(
    Object.keys(files).map((k) => [k, "/proj/" + k]),
  ),
  dirs,
});

describe("reconcileScan", () => {
  it("adopts the scan as the project map (added appear, deleted drop)", () => {
    const r = reconcileScan(
      scan({ "tune.ctab": "score {}", "new.ctab": "def x() {}" }),
      () => undefined, // nothing open
    );
    expect(Object.keys(r.files)).toEqual(["tune.ctab", "new.ctab"]);
    expect(r.filePaths["new.ctab"]).toBe("/proj/new.ctab");
    expect(r.reloads).toEqual([]);
  });

  it("queues a reload when an open file's disk content diverged", () => {
    const open: Record<string, string> = { "tune.ctab": "OLD" };
    const r = reconcileScan(
      scan({ "tune.ctab": "NEW", "lib.ctab": "def x() {}" }),
      (key) => open[key],
    );
    // Only the open, diverged file reloads; the unopened one doesn't.
    expect(r.reloads).toEqual([{ key: "tune.ctab", content: "NEW" }]);
  });

  it("reloads even a file whose buffer has unsaved edits (always-reload)", () => {
    // The buffer ("DIRTY") differs from disk ("DISK") — disk still wins.
    const r = reconcileScan(scan({ "tune.ctab": "DISK" }), () => "DIRTY");
    expect(r.reloads).toEqual([{ key: "tune.ctab", content: "DISK" }]);
  });

  it("does not reload an open file already matching disk", () => {
    const r = reconcileScan(scan({ "tune.ctab": "SAME" }), () => "SAME");
    expect(r.reloads).toEqual([]);
  });

  it("carries the scan's directory keys through (empty folders persist)", () => {
    const r = reconcileScan(
      scan({ "licks/roll.ctab": "x" }, ["licks", "empty"]),
      () => undefined,
    );
    expect(r.dirs).toEqual(["licks", "empty"]);
  });
});
