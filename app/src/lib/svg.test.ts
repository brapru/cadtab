import { describe, it, expect } from "vitest";
import { renderTreeToSvg } from "./svg";
import type { RenderTree, Primitive } from "./types";

function tree(prims: Primitive[]): RenderTree {
  return {
    meta: { width: 10, height: 4 },
    header: [],
    systems: [
      {
        bounds: { x: 0, y: 0, w: 10, h: 4 },
        prims: [],
        measures: [{ bounds: { x: 0, y: 0, w: 10, h: 4 }, prims, span: null }],
      },
    ],
  };
}

const text = (
  over: Partial<Extract<Primitive, { kind: "text" }>>,
): Primitive => ({
  kind: "text",
  x: 1,
  y: 2,
  content: "0",
  role: "fretNumber",
  span: null,
  ...over,
});

describe("renderTreeToSvg", () => {
  it("emits a well-formed standalone SVG with a viewBox and background", () => {
    const svg = renderTreeToSvg(tree([]));
    expect(svg.startsWith("<svg")).toBe(true);
    expect(svg).toContain('xmlns="http://www.w3.org/2000/svg"');
    expect(svg).toContain('viewBox="0 0 10 4"');
    // Scaled pixel dimensions (10*16 x 4*16).
    expect(svg).toContain('width="160"');
    expect(svg).toContain('height="64"');
    // Opaque sheet background, then the closing tag.
    expect(svg).toContain(
      '<rect x="0" y="0" width="10" height="4" fill="#ffffff"/>',
    );
    expect(svg.trimEnd().endsWith("</svg>")).toBe(true);
  });

  it("draws a fret number as centered ink text at the right size", () => {
    const svg = renderTreeToSvg(
      tree([text({ content: "7", role: "fretNumber" })]),
    );
    expect(svg).toContain("<text");
    expect(svg).toContain('font-size="1.3"');
    expect(svg).toContain('text-anchor="middle"');
    expect(svg).toContain('fill="#1a1a1a"');
    expect(svg).toContain(">7</text>");
  });

  it("draws muted, start-anchored header text for the tuning block", () => {
    const svg = renderTreeToSvg(
      tree([text({ content: "Open G", role: "tuningName" })]),
    );
    expect(svg).toContain('text-anchor="start"');
    expect(svg).toContain('fill="#6b6b6b"');
  });

  it("italicizes techniques and bolds the title", () => {
    const svg = renderTreeToSvg(
      tree([
        text({ content: "h", role: "technique" }),
        text({ content: "Tune", role: "title" }),
      ]),
    );
    expect(svg).toContain('font-style="italic"');
    expect(svg).toContain('font-weight="600"');
  });

  it("draws lines and paths with ink strokes", () => {
    const svg = renderTreeToSvg(
      tree([
        { kind: "line", x1: 0, y1: 0, x2: 10, y2: 0, weight: 0.1 },
        { kind: "path", cmds: "M0 0 L1 1", span: null },
      ]),
    );
    expect(svg).toContain(
      '<line x1="0" y1="0" x2="10" y2="0" stroke="#1a1a1a" stroke-width="0.1" stroke-linecap="butt"/>',
    );
    expect(svg).toContain('<path d="M0 0 L1 1" fill="none" stroke="#1a1a1a"');
    expect(svg).toContain('stroke-linecap="round"');
  });

  it("XML-escapes text content and path data", () => {
    const svg = renderTreeToSvg(
      tree([
        text({ content: "a & b <c>" }),
        { kind: "path", cmds: 'M0 0 "x"', span: null },
      ]),
    );
    expect(svg).toContain(">a &amp; b &lt;c&gt;</text>");
    expect(svg).toContain('d="M0 0 &quot;x&quot;"');
  });

  it("produces well-formed XML that parses without error", () => {
    const svg = renderTreeToSvg(
      tree([
        text({ content: "a & b", role: "title" }),
        { kind: "line", x1: 0, y1: 0, x2: 1, y2: 1, weight: 0.1 },
        { kind: "path", cmds: "M0 0 L1 1", span: null },
      ]),
    );
    const doc = new DOMParser().parseFromString(svg, "image/svg+xml");
    expect(doc.querySelector("parsererror")).toBeNull();
    expect(doc.documentElement.tagName).toBe("svg");
    expect(doc.querySelectorAll("text, line, path").length).toBe(3);
  });

  it("includes header and system-furniture primitives, not just measures", () => {
    const t = tree([]);
    t.header = [text({ content: "Title", role: "title" })];
    t.systems[0].prims = [text({ content: "g", role: "stringLabel" })];
    const svg = renderTreeToSvg(t);
    expect(svg).toContain(">Title</text>");
    expect(svg).toContain(">g</text>");
  });
});
