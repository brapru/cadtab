// Logical layout units per CSS pixel at base zoom. The width handed to the
// layout engine is the render pane's pixel width scaled into string-spacing
// units, so a wider pane reflows into more measures per system while glyphs keep
// a steady size.
export const PX_PER_UNIT = 12;
export const MIN_LAYOUT_WIDTH = 12;

// Logical units across a printable page's content area — the justify target for
// PDF export and the print preview (both paginate to this so they match). Sets
// the measures-per-line density; provisional, eyeballed against a Letter page.
export const PDF_CONTENT_WIDTH = 80;

export function layoutWidthForPx(px: number, pxPerUnit = PX_PER_UNIT): number {
  return Math.max(MIN_LAYOUT_WIDTH, px / pxPerUnit);
}

// Visual zoom of the rendered tab (a CSS scale on top of layout). Zoom 1 fits
// the pane width, since layout already reflows to it; in/out scale from there.
export const MIN_ZOOM = 0.25;
export const MAX_ZOOM = 4;
export const ZOOM_STEP = 1.2;

export function clampZoom(z: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));
}
