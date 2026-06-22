import { describe, it, expect } from "vitest";
import {
  spanCoversByte,
  spansOverlap,
  primitiveSpans,
  narrowestSpanAt,
} from "./mapping";
import type { RenderTree } from "./types";

const rect = { x: 0, y: 0, w: 10, h: 5 };

// A tree with a narrow note span (0..3) nested inside a wider path span (0..10),
// plus a second note (5..8). A line carries no span and must be ignored.
const tree: RenderTree = {
  meta: { width: 10, height: 5 },
  header: [],
  systems: [
    {
      bounds: rect,
      prims: [],
      measures: [
        {
          bounds: rect,
          span: null,
          prims: [
            {
              kind: "text",
              x: 1,
              y: 1,
              content: "3",
              role: "fretNumber",
              span: { start: 0, end: 3 },
            },
            { kind: "path", cmds: "M0 0", span: { start: 0, end: 10 } },
            { kind: "line", x1: 0, y1: 0, x2: 1, y2: 0, weight: 0.1 },
            {
              kind: "text",
              x: 2,
              y: 1,
              content: "5",
              role: "fretNumber",
              span: { start: 5, end: 8 },
            },
          ],
        },
      ],
    },
  ],
};

describe("spanCoversByte", () => {
  it("is end-inclusive", () => {
    expect(spanCoversByte({ start: 0, end: 3 }, 0)).toBe(true);
    expect(spanCoversByte({ start: 0, end: 3 }, 3)).toBe(true);
    expect(spanCoversByte({ start: 0, end: 3 }, 4)).toBe(false);
  });
});

describe("spansOverlap", () => {
  it("is true when ranges intersect, false when they merely touch", () => {
    expect(spansOverlap({ start: 0, end: 3 }, { start: 2, end: 5 })).toBe(true);
    expect(spansOverlap({ start: 0, end: 3 }, { start: 3, end: 5 })).toBe(
      false,
    );
  });
});

describe("primitiveSpans", () => {
  it("collects text and path spans, skipping spanless lines", () => {
    expect(primitiveSpans(tree)).toEqual([
      { start: 0, end: 3 },
      { start: 0, end: 10 },
      { start: 5, end: 8 },
    ]);
  });
});

describe("narrowestSpanAt", () => {
  it("prefers the tightest span covering the byte", () => {
    expect(narrowestSpanAt(tree, 1)).toEqual({ start: 0, end: 3 });
    expect(narrowestSpanAt(tree, 6)).toEqual({ start: 5, end: 8 });
  });

  it("falls back to a wider span when no tighter one covers the byte", () => {
    expect(narrowestSpanAt(tree, 9)).toEqual({ start: 0, end: 10 });
  });

  it("returns null when nothing covers the byte", () => {
    expect(narrowestSpanAt(tree, 50)).toBeNull();
  });
});
