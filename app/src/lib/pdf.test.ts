/// <reference types="node" />
// Node types are opted in here (this test reads the vendored font files from
// disk) without making them global — browser source keeps its jsdom purity.
import { describe, it, expect, beforeAll, afterAll, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { PDFDocument } from "pdf-lib";
import { paginatedTreeToPdf } from "./pdf";
import type { PaginatedTree, Page, Primitive, TextRole } from "./types";

// pdf.ts fetches its embedded fonts from /fonts/*.woff (served from public/ in the
// app). In jsdom there is no server, so back `fetch` with the real vendored files
// on disk — this exercises the genuine subset-embedding path, no font stubs.
// Vitest runs with cwd at the app root, where public/fonts lives.
const FONT_DIR = resolve(process.cwd(), "public/fonts") + "/";

beforeAll(() => {
  vi.stubGlobal("fetch", async (url: string) => {
    const name = url.split("/").pop()!;
    const bytes = new Uint8Array(readFileSync(FONT_DIR + name));
    return { arrayBuffer: async () => bytes };
  });
});

afterAll(() => vi.unstubAllGlobals());

function text(role: TextRole, content: string, x = 5, y = 5): Primitive {
  return { kind: "text", x, y, content, role, span: null };
}

function page(extra: Primitive[] = []): Page {
  return {
    bounds: { x: 0, y: 0, w: 80, h: 103.5 },
    header: [text("title", "Cripple Creek")],
    systems: [
      {
        bounds: { x: 0, y: 10, w: 80, h: 4 },
        prims: [
          { kind: "line", x1: 2, y1: 10, x2: 78, y2: 10, weight: 0.06 },
          ...extra,
        ],
        measures: [
          {
            bounds: { x: 2, y: 10, w: 20, h: 4 },
            prims: [
              text("fretNumber", "0", 6, 10),
              text("fretNumber", "12", 12, 11),
            ],
            span: null,
          },
        ],
      },
    ],
  };
}

describe("paginatedTreeToPdf", () => {
  it("emits valid PDF bytes with one PDF page per tree page", async () => {
    const tree: PaginatedTree = {
      pageWidth: 80,
      pageHeight: 103.5,
      pages: [page(), { ...page(), header: [text("pageNumber", "2", 78, 1)] }],
    };

    const bytes = await paginatedTreeToPdf(tree);

    // Header + trailer: a real PDF starts with `%PDF-` and ends with `%%EOF`.
    const head = new TextDecoder().decode(bytes.slice(0, 5));
    expect(head).toBe("%PDF-");
    const tail = new TextDecoder().decode(bytes.slice(-6));
    expect(tail.trim().endsWith("%%EOF")).toBe(true);

    // Page count matches the tree (re-parse rather than trust raw bytes).
    const reloaded = await PDFDocument.load(bytes);
    expect(reloaded.getPageCount()).toBe(2);
  });

  it("embeds the special glyph roles without throwing", async () => {
    // A rest, a tempo note, a circled tuning digit, and a strum arrow exercise the
    // music font + vector glyph paths.
    const tree: PaginatedTree = {
      pageWidth: 80,
      pageHeight: 103.5,
      pages: [
        page([
          text("rest", "\u{1D13D}", 20, 11),
          text("tempo", "♩ = 120", 40, 3),
          text("tuningString", "①=D", 4, 4),
          text("strum", "↓", 30, 14),
          { kind: "path", cmds: "M 6 9 Q 9 7 12 9", span: null },
        ]),
      ],
    };

    const bytes = await paginatedTreeToPdf(tree);
    expect(new TextDecoder().decode(bytes.slice(0, 5))).toBe("%PDF-");
    const reloaded = await PDFDocument.load(bytes);
    expect(reloaded.getPageCount()).toBe(1);
  });

  it("handles the glyph fallbacks and A4 size", async () => {
    // Up-strum arrow, a tempo with no leading note, and a >20 tuning cell (the
    // "(21)=X" ASCII fallback) all take the non-default branches.
    const tree: PaginatedTree = {
      pageWidth: 80,
      pageHeight: 113.1,
      pages: [
        page([
          text("strum", "↑", 30, 14),
          text("tempo", "120", 40, 3),
          text("tuningString", "(21)=X", 4, 4),
        ]),
      ],
    };
    const bytes = await paginatedTreeToPdf(tree, "a4");
    const reloaded = await PDFDocument.load(bytes);
    expect(reloaded.getPageCount()).toBe(1);
  });

  it("paginates an empty document to a single page", async () => {
    const tree: PaginatedTree = {
      pageWidth: 80,
      pageHeight: 103.5,
      pages: [
        { bounds: { x: 0, y: 0, w: 80, h: 103.5 }, header: [], systems: [] },
      ],
    };
    const bytes = await paginatedTreeToPdf(tree);
    const reloaded = await PDFDocument.load(bytes);
    expect(reloaded.getPageCount()).toBe(1);
  });
});
