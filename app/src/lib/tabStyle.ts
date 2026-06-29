import type { TextRole } from "./types";

// The single source of truth for how each render-tree text role is drawn, shared
// by the live painter (Tab.svelte) and the standalone SVG exporter (svg.ts) so
// the two never drift. Sizes are in logical units (1 unit = string spacing).

export type TextStyle = { size: number; weight?: number; italic?: boolean };

export const TEXT_STYLE: Record<TextRole, TextStyle> = {
  title: { size: 2.2, weight: 600 },
  composer: { size: 1.5, weight: 700 },
  tuningName: { size: 1.2 },
  tuningString: { size: 1.2 },
  tempo: { size: 1.2, weight: 700 },
  capo: { size: 1.2 },
  fretNumber: { size: 1.3 },
  stringLabel: { size: 1.1 },
  timeSig: { size: 1.4, weight: 600 },
  finger: { size: 0.95 },
  strum: { size: 1.5 },
  technique: { size: 0.95, italic: true },
  ending: { size: 0.95 },
  rest: { size: 1.5 },
  sectionLabel: { size: 1.2, weight: 700 },
  chordSymbol: { size: 1.05, weight: 600 },
  barNumber: { size: 1.0 },
  defHeading: { size: 1.1, weight: 700 },
  defNote: { size: 0.8, italic: true },
  pageNumber: { size: 0.8 },
};

// The left-aligned header block anchors at its start x rather than centring.
const START_ANCHORED: ReadonlySet<TextRole> = new Set([
  "tuningName",
  "tuningString",
  "capo",
  "sectionLabel",
  "barNumber",
  "defHeading",
  "defNote",
]);

// The folio page number (T7.19) sits flush against the right margin.
const END_ANCHORED: ReadonlySet<TextRole> = new Set(["pageNumber"]);

// Hand/technique annotations and small above-staff marks read as secondary ink.
// The header block (tuning name, string grid, capo) is primary ink — full black.
const MUTED_ROLES: ReadonlySet<TextRole> = new Set([
  "finger",
  "technique",
  "strum",
  "ending",
  "barNumber",
  "defNote",
  "pageNumber",
]);

export function textAnchor(role: TextRole): "start" | "middle" | "end" {
  if (END_ANCHORED.has(role)) return "end";
  return START_ANCHORED.has(role) ? "start" : "middle";
}

export function isMuted(role: TextRole): boolean {
  return MUTED_ROLES.has(role);
}

// Open curves (ties, slides, bends, choke arcs) stroke at a hairline weight.
export const PATH_STROKE_WIDTH = 0.07;

// The tempo mark's leading note glyph (♩) renders smaller than its em, so the
// painters draw it at this multiple of the tempo text size to match the "= NNN"
// visually. Shared by svg.ts (screen/preview) and pdf.ts (export).
export const TEMPO_NOTE_BOOST = 1.6;
export const TEMPO_NOTE = "♩";

// The engraved-sheet serif stack used across all rendered text. Source Serif 4 is
// self-hosted (app.css) and the same family is embedded into PDF exports, so the
// on-screen tab and the printed page render in one identical face; Georgia/serif
// are fallbacks only until the webfont loads.
export const FONT_FAMILY = "'Source Serif 4', Georgia, serif";
