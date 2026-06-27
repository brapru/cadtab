import type { TextRole } from "./types";

// The single source of truth for how each render-tree text role is drawn, shared
// by the live painter (Tab.svelte) and the standalone SVG exporter (svg.ts) so
// the two never drift. Sizes are in logical units (1 unit = string spacing).

export type TextStyle = { size: number; weight?: number; italic?: boolean };

export const TEXT_STYLE: Record<TextRole, TextStyle> = {
  title: { size: 1.5, weight: 600 },
  composer: { size: 1.0, weight: 700 },
  tuningName: { size: 0.85 },
  tuningString: { size: 0.85 },
  tempo: { size: 0.9, weight: 700 },
  capo: { size: 0.85 },
  fretNumber: { size: 1.3 },
  stringLabel: { size: 1.1 },
  timeSig: { size: 1.4, weight: 600 },
  finger: { size: 0.95 },
  strum: { size: 1.5 },
  technique: { size: 0.95, italic: true },
  ending: { size: 0.95 },
  rest: { size: 1.5 },
  sectionLabel: { size: 1.2, weight: 700 },
};

// The left-aligned header block anchors at its start x rather than centring.
const START_ANCHORED: ReadonlySet<TextRole> = new Set([
  "tuningName",
  "tuningString",
  "capo",
  "sectionLabel",
]);

// Hand/technique annotations and the header tuning block read as secondary ink.
const MUTED_ROLES: ReadonlySet<TextRole> = new Set([
  "finger",
  "technique",
  "strum",
  "ending",
  "tuningName",
  "tuningString",
  "capo",
]);

export function textAnchor(role: TextRole): "start" | "middle" {
  return START_ANCHORED.has(role) ? "start" : "middle";
}

export function isMuted(role: TextRole): boolean {
  return MUTED_ROLES.has(role);
}

// Open curves (ties, slides, bends, choke arcs) stroke at a hairline weight.
export const PATH_STROKE_WIDTH = 0.07;

// The engraved-sheet serif stack used across all rendered text.
export const FONT_FAMILY = 'Georgia, "Times New Roman", serif';
