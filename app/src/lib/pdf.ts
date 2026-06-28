import type { PDFDocument, PDFFont, PDFPage, RGB } from "pdf-lib";
import type { PaginatedTree, Page, Primitive, TextRole } from "./types";
import { TEXT_STYLE, textAnchor, isMuted, PATH_STROKE_WIDTH } from "./tabStyle";

// Vector PDF export (T7.19b): paint a `PaginatedTree` straight into a PDF with
// real text and vector strokes — crisp at any zoom, small files, identical on
// desktop (WKWebView) and web. Styling decisions come from tabStyle.ts (the one
// source of truth shared with the live painter and the SVG exporter), so the PDF
// matches the screen. Fonts are self-hosted and embedded/subset into the file.
//
// The render emits a few glyphs the embedded serif lacks: circled tuning digits
// and strum arrows are drawn as vector primitives here; musical rests and the
// tempo note come from an embedded Noto Music subset. pdf-lib + fontkit do the
// font subsetting (it is not something to hand-roll), and both are dynamically
// imported so they never touch the app's initial bundle.

// Printable sheet ink: dark on white, secondary ink for annotations. Fixed, not
// themed — an export is a shareable artifact. (Mirrors svg.ts.)
const INK_HEX = { r: 0x1a, g: 0x1a, b: 0x1a };
const MUTED_HEX = { r: 0x6b, g: 0x6b, b: 0x6b };

// Physical page sizes in PostScript points (72pt = 1in). The logical page box
// (logical units) maps onto this; the aspect ratios match `PageSize` in core.
const POINTS = {
  letter: { w: 612, h: 792 },
  a4: { w: 595.28, h: 841.89 },
};

// Font asset URLs (self-hosted; see app.css / public/fonts). WOFF (not WOFF2) for
// embedding — fontkit decodes WOFF1 but not WOFF2's Brotli.
const FONT_URLS = {
  regular: "/fonts/SourceSerif4-Regular.woff",
  semibold: "/fonts/SourceSerif4-SemiBold.woff",
  bold: "/fonts/SourceSerif4-Bold.woff",
  italic: "/fonts/SourceSerif4-Italic.woff",
  music: "/fonts/NotoMusic.woff",
};

type Fonts = Record<keyof typeof FONT_URLS, PDFFont>;

// Cap-height fraction of font size, used to place text so its visual centre sits
// at the target y — matching the SVG painter's `dominant-baseline: central`.
// Source Serif 4's cap height is ~0.66em; centre is half that above the baseline.
const CAP_CENTER = 0.66 / 2;

/// Serialize a paginated document to PDF bytes. Each `Page` becomes one PDF page
/// drawn in its own coordinate space.
export async function paginatedTreeToPdf(
  tree: PaginatedTree,
  pageSize: "letter" | "a4" = "letter",
): Promise<Uint8Array> {
  const { PDFDocument, rgb } = await import("pdf-lib");
  const fontkit = (await import("@pdf-lib/fontkit")).default;

  const doc = await PDFDocument.create();
  doc.registerFontkit(fontkit);
  const fonts = await embedFonts(doc);

  const pt = POINTS[pageSize];
  for (const page of tree.pages) {
    const pdfPage = doc.addPage([pt.w, pt.h]);
    paintPage(pdfPage, page, pt.w, pt.h, fonts, rgb);
  }

  return doc.save();
}

async function embedFonts(doc: PDFDocument): Promise<Fonts> {
  const entries = await Promise.all(
    (Object.keys(FONT_URLS) as (keyof typeof FONT_URLS)[]).map(async (key) => {
      const bytes = await (await fetch(FONT_URLS[key])).arrayBuffer();
      return [key, await doc.embedFont(bytes, { subset: true })] as const;
    }),
  );
  return Object.fromEntries(entries) as Fonts;
}

// One page painter. Logical coordinates are y-down from the top-left; PDF is
// y-up from the bottom-left, so every y flips. A uniform `scale` maps the logical
// page box onto the physical point size (paginate guarantees matching aspect).
function paintPage(
  pdfPage: PDFPage,
  page: Page,
  wPt: number,
  hPt: number,
  fonts: Fonts,
  rgb: (r: number, g: number, b: number) => RGB,
): void {
  const scale = wPt / page.bounds.w;
  const ink = rgb(INK_HEX.r / 255, INK_HEX.g / 255, INK_HEX.b / 255);
  const muted = rgb(MUTED_HEX.r / 255, MUTED_HEX.g / 255, MUTED_HEX.b / 255);
  const flipY = (y: number) => hPt - y * scale;

  const prims: Primitive[] = [
    ...page.header,
    ...page.systems.flatMap((s) => [
      ...s.prims,
      ...s.measures.flatMap((m) => m.prims),
    ]),
  ];

  for (const p of prims) {
    if (p.kind === "line") {
      pdfPage.drawLine({
        start: { x: p.x1 * scale, y: flipY(p.y1) },
        end: { x: p.x2 * scale, y: flipY(p.y2) },
        thickness: p.weight * scale,
        color: ink,
      });
    } else if (p.kind === "path") {
      // drawSvgPath interprets the path in SVG space (y-down) anchored at the
      // given origin, so place the SVG origin at the page's top-left and scale.
      pdfPage.drawSvgPath(p.cmds, {
        x: 0,
        y: hPt,
        scale,
        borderColor: ink,
        borderWidth: PATH_STROKE_WIDTH * scale,
      });
    } else {
      drawText(pdfPage, p, scale, flipY, fonts, ink, muted);
    }
  }
}

// Draw one text primitive, dispatching the few non-Latin roles to vector glyphs
// or the music font, and everything else to the embedded serif.
function drawText(
  pdfPage: PDFPage,
  p: Extract<Primitive, { kind: "text" }>,
  scale: number,
  flipY: (y: number) => number,
  fonts: Fonts,
  ink: RGB,
  muted: RGB,
): void {
  const color = isMuted(p.role) ? muted : ink;
  const size = TEXT_STYLE[p.role].size * scale;
  const x = p.x * scale;
  const y = flipY(p.y);

  if (p.role === "strum") {
    drawStrumArrow(pdfPage, p.content, x, flipY(p.y), size, ink);
    return;
  }
  if (p.role === "rest") {
    // A single musical rest glyph from the embedded music font, centred.
    drawCentral(pdfPage, p.content, fonts.music, size, x, y, color, "middle");
    return;
  }
  if (p.role === "tuningString") {
    drawTuningString(pdfPage, p.content, x, y, size, fonts, color);
    return;
  }
  if (p.role === "tempo") {
    drawTempo(pdfPage, p.content, x, y, size, fonts, color);
    return;
  }
  drawCentral(
    pdfPage,
    p.content,
    fontFor(p.role, fonts),
    size,
    x,
    y,
    color,
    textAnchor(p.role),
  );
}

// Place text so its visual centre sits at (anchorX, centerY), matching the SVG
// painter's anchor + `dominant-baseline: central`.
function drawCentral(
  pdfPage: PDFPage,
  text: string,
  font: PDFFont,
  size: number,
  anchorX: number,
  centerY: number,
  color: RGB,
  anchor: "start" | "middle" | "end",
): void {
  if (text === "") return;
  const width = safeWidth(font, text, size);
  const x =
    anchor === "middle"
      ? anchorX - width / 2
      : anchor === "end"
        ? anchorX - width
        : anchorX;
  pdfPage.drawText(text, {
    x,
    y: centerY - CAP_CENTER * size,
    size,
    font,
    color,
  });
}

// The Latin serif face for a role's weight/italic (rest/strum/tuning/tempo are
// handled before this is reached).
function fontFor(role: TextRole, fonts: Fonts): PDFFont {
  const style = TEXT_STYLE[role];
  if (style.italic) return fonts.italic;
  if ((style.weight ?? 400) >= 700) return fonts.bold;
  if ((style.weight ?? 400) >= 600) return fonts.semibold;
  return fonts.regular;
}

// A tuning cell "①=D": draw the circled digit as a vector circle + plain digit
// (crisper and font-independent than relying on enclosed-alphanumerics glyphs),
// then the "=D" remainder in serif. The ">20" fallback "(21)=X" is plain ASCII.
function drawTuningString(
  pdfPage: PDFPage,
  content: string,
  x: number,
  centerY: number,
  size: number,
  fonts: Fonts,
  color: RGB,
): void {
  const first = content.codePointAt(0) ?? 0;
  if (first < 0x2460 || first > 0x2473) {
    drawCentral(
      pdfPage,
      content,
      fonts.regular,
      size,
      x,
      centerY,
      color,
      "start",
    );
    return;
  }
  const digit = String(first - 0x2460 + 1);
  const r = size * 0.46;
  const cx = x + r;
  pdfPage.drawCircle({
    x: cx,
    y: centerY,
    size: r,
    borderColor: color,
    borderWidth: size * 0.06,
  });
  drawCentral(
    pdfPage,
    digit,
    fonts.regular,
    size * 0.72,
    cx,
    centerY,
    color,
    "middle",
  );
  // Remainder ("=D") starts just past the circle.
  const rest = [...content].slice(1).join("");
  drawCentral(
    pdfPage,
    rest,
    fonts.regular,
    size,
    cx + r + size * 0.12,
    centerY,
    color,
    "start",
  );
}

// Tempo "♩ = 120": the leading note from the music font, then " = 120" in serif.
function drawTempo(
  pdfPage: PDFPage,
  content: string,
  x: number,
  centerY: number,
  size: number,
  fonts: Fonts,
  color: RGB,
): void {
  const chars = [...content];
  if (chars[0] !== "♩") {
    drawCentral(
      pdfPage,
      content,
      fonts.regular,
      size,
      x,
      centerY,
      color,
      "middle",
    );
    return;
  }
  const rest = chars.slice(1).join("");
  // Centre the whole "♩…" string on x: measure the note (music font) + rest (serif).
  const noteW = safeWidth(fonts.music, "♩", size);
  const restW = safeWidth(fonts.regular, rest, size);
  let cursor = x - (noteW + restW) / 2;
  pdfPage.drawText("♩", {
    x: cursor,
    y: centerY - CAP_CENTER * size,
    size,
    font: fonts.music,
    color,
  });
  cursor += noteW;
  pdfPage.drawText(rest, {
    x: cursor,
    y: centerY - CAP_CENTER * size,
    size,
    font: fonts.regular,
    color,
  });
}

// A strum direction as a vertical vector arrow centred on x: a shaft plus a
// chevron head, all stroked lines (no fills) so it stays crisp. PDF is y-up, so
// a down-strum (↓) tips at the bottom, an up-strum (↑) at the top.
function drawStrumArrow(
  pdfPage: PDFPage,
  content: string,
  x: number,
  centerY: number,
  size: number,
  ink: RGB,
): void {
  const down = content !== "↑";
  const half = size * 0.42;
  const top = centerY + half; // PDF y-up: larger y is higher
  const bottom = centerY - half;
  const tip = down ? bottom : top;
  const tail = down ? top : bottom;
  const headLen = size * 0.34;
  const headW = size * 0.22;
  const thickness = size * 0.08;
  // Shaft, then the two chevron wings running from the tip back toward the tail.
  const wingY = down ? tip + headLen : tip - headLen;
  const line = (x1: number, y1: number, x2: number, y2: number) =>
    pdfPage.drawLine({
      start: { x: x1, y: y1 },
      end: { x: x2, y: y2 },
      thickness,
      color: ink,
    });
  line(x, tail, x, tip);
  line(x, tip, x - headW, wingY);
  line(x, tip, x + headW, wingY);
}

// pdf-lib throws if a glyph is missing from a subset font; tab text is plain
// Latin/digits so this is just defensive (returns 0 width on the rare miss).
function safeWidth(font: PDFFont, text: string, size: number): number {
  try {
    return font.widthOfTextAtSize(text, size);
  } catch {
    return 0;
  }
}
