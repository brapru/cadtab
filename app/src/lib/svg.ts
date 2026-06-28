import type { RenderTree, Primitive } from "./types";
import {
  TEXT_STYLE,
  textAnchor,
  isMuted,
  PATH_STROKE_WIDTH,
  FONT_FAMILY,
} from "./tabStyle";

// Serialize a render tree to a standalone SVG string for export: self-contained
// (concrete colours and inline styles, no CSS variables or external sheet) so it
// renders the same wherever it is opened or rasterized. The live painter
// (Tab.svelte) shares the role styling via tabStyle.ts, so export matches screen.

// A printable sheet: dark ink on white, secondary ink for annotations. Fixed
// rather than themed — an export is a shareable artifact, not the live UI.
const BG = "#ffffff";
const INK = "#1a1a1a";
const MUTED = "#6b6b6b";

// Logical units (1 = string spacing) are scaled to pixels for the SVG's width/
// height so exported text is legible; the viewBox keeps the logical coordinates.
export const EXPORT_SCALE = 16;

function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// Round to 3 decimals to drop f32 serialization noise and keep the SVG compact.
function num(n: number): string {
  return String(Math.round(n * 1000) / 1000);
}

function primitiveToSvg(p: Primitive): string {
  if (p.kind === "line") {
    return (
      `<line x1="${num(p.x1)}" y1="${num(p.y1)}" x2="${num(p.x2)}" y2="${num(p.y2)}"` +
      ` stroke="${INK}" stroke-width="${num(p.weight)}" stroke-linecap="butt"/>`
    );
  }
  if (p.kind === "path") {
    return (
      `<path d="${esc(p.cmds)}" fill="none" stroke="${INK}"` +
      ` stroke-width="${PATH_STROKE_WIDTH}" stroke-linecap="round"/>`
    );
  }
  const style = TEXT_STYLE[p.role];
  const attrs = [
    `x="${num(p.x)}"`,
    `y="${num(p.y)}"`,
    `font-size="${style.size}"`,
    style.weight ? `font-weight="${style.weight}"` : "",
    style.italic ? `font-style="italic"` : "",
    `text-anchor="${textAnchor(p.role)}"`,
    `dominant-baseline="central"`,
    `fill="${isMuted(p.role) ? MUTED : INK}"`,
  ]
    .filter(Boolean)
    .join(" ");
  return `<text ${attrs}>${esc(p.content)}</text>`;
}

/// Render `tree` to a complete, standalone SVG document string.
export function renderTreeToSvg(tree: RenderTree): string {
  const { width: w, height: h } = tree.meta;
  const body: string[] = [];
  for (const p of tree.header) body.push(primitiveToSvg(p));
  for (const system of tree.systems) {
    for (const p of system.prims) body.push(primitiveToSvg(p));
    for (const measure of system.measures) {
      for (const p of measure.prims) body.push(primitiveToSvg(p));
    }
  }
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${num(w)} ${num(h)}"` +
      ` width="${num(w * EXPORT_SCALE)}" height="${num(h * EXPORT_SCALE)}"` +
      // Double-quoted: FONT_FAMILY single-quotes the face name ('Source Serif 4').
      ` font-family="${FONT_FAMILY}">`,
    `<rect x="0" y="0" width="${num(w)}" height="${num(h)}" fill="${BG}"/>`,
    ...body,
    `</svg>`,
  ].join("\n");
}
