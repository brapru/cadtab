import { StateField, StateEffect } from "@codemirror/state";
import { Decoration, EditorView, hoverTooltip } from "@codemirror/view";
import type { DecorationSet } from "@codemirror/view";
import type { Diagnostic, Severity, Span } from "./types";
import { byteToCharIndex, spanToRange, type CharRange } from "./spans";

// Problem counts by severity, for the bottom bar's diagnostics indicator (info
// diagnostics don't count as problems).
export interface DiagnosticCounts {
  errors: number;
  warnings: number;
}

export function diagnosticCounts(diagnostics: Diagnostic[]): DiagnosticCounts {
  let errors = 0;
  let warnings = 0;
  for (const d of diagnostics) {
    if (d.severity === "error") errors++;
    else if (d.severity === "warning") warnings++;
  }
  return { errors, warnings };
}

// A diagnostic positioned in CodeMirror (UTF-16) coordinates, ready to underline
// and to surface in a hover tooltip.
export interface PlacedDiagnostic {
  from: number;
  to: number;
  severity: Severity;
  message: string;
  help: string | null;
}

// Resolve diagnostic byte spans against the source, dropping any out of range
// (e.g. left over from a stale compile of longer text). A zero-width span — an
// "expected `}` at end of input"-style error that points between characters —
// can't be underlined as-is, so it falls back to the character just before the
// point, keeping every counted problem visible as a squiggle.
export function placeDiagnostics(
  source: string,
  diagnostics: Diagnostic[],
): PlacedDiagnostic[] {
  const map = byteToCharIndex(source);
  const placed: PlacedDiagnostic[] = [];
  for (const d of diagnostics) {
    const r = spanToRange(map, d.span) ?? pointFallback(map, d.span);
    if (!r) continue;
    placed.push({
      ...r,
      severity: d.severity,
      message: d.message,
      help: d.help,
    });
  }
  return placed;
}

// Give a zero-width diagnostic something to underline: the character before the
// point (or the first character when the point is at the very start). Only
// applies to genuinely empty spans — a non-empty span that didn't resolve is out
// of range and stays dropped.
function pointFallback(map: number[], span: Span): CharRange | null {
  if (span.start !== span.end) return null;
  const at = map[span.start];
  if (at === undefined) return null;
  if (at > 0) return { from: at - 1, to: at };
  return map.length > 1 ? { from: 0, to: 1 } : null;
}

// A diagnostic prepared for the bottom-bar problems panel: the original byte
// span (for the jump), plus a 1-based line/column for display. `inRange` is false
// for a span left over from a stale compile of longer text — it's still listed
// (the bottom-bar count includes it) but can't be jumped to.
export interface DiagnosticEntry {
  severity: Severity;
  message: string;
  help: string | null;
  span: Span;
  line: number;
  col: number;
  inRange: boolean;
}

// 1-based line/column of a char offset, clamped into the source.
function lineCol(
  source: string,
  charIndex: number,
): { line: number; col: number } {
  const at = Math.min(Math.max(charIndex, 0), source.length);
  let line = 1;
  let col = 1;
  for (let i = 0; i < at; i++) {
    if (source[i] === "\n") {
      line++;
      col = 1;
    } else {
      col++;
    }
  }
  return { line, col };
}

// Build the problems-panel list: every diagnostic with a display location, sorted
// by source position so the list reads top-to-bottom like the squiggles. Matches
// the bottom-bar count (which tallies all diagnostics), so out-of-range stale
// spans are kept and flagged rather than dropped.
export function diagnosticEntries(
  source: string,
  diagnostics: Diagnostic[],
): DiagnosticEntry[] {
  const map = byteToCharIndex(source);
  const entries = diagnostics.map((d) => {
    const charStart = map[d.span.start];
    const inRange = charStart !== undefined;
    const { line, col } = lineCol(source, inRange ? charStart : 0);
    return {
      severity: d.severity,
      message: d.message,
      help: d.help,
      span: d.span,
      line,
      col,
      inRange,
    };
  });
  entries.sort((a, b) => a.span.start - b.span.start);
  return entries;
}

// The diagnostics covering a position, for the hover tooltip.
export function diagnosticsAt(
  placed: PlacedDiagnostic[],
  pos: number,
): PlacedDiagnostic[] {
  return placed.filter((p) => pos >= p.from && pos <= p.to);
}

function decorations(placed: PlacedDiagnostic[]): DecorationSet {
  const marks = placed.map((p) =>
    Decoration.mark({ class: `cm-diag-${p.severity}` }).range(p.from, p.to),
  );
  return Decoration.set(marks, true);
}

// Effect carrying the latest diagnostics for the current document.
export const setDiagnostics = StateEffect.define<Diagnostic[]>();

// Holds the placed diagnostics: rebuilt when fresh diagnostics arrive, and
// position-mapped through edits so squiggles and tooltips track the text between
// recompiles.
export const diagnosticsField = StateField.define<PlacedDiagnostic[]>({
  create() {
    return [];
  },
  update(placed, tr) {
    for (const e of tr.effects) {
      if (e.is(setDiagnostics)) {
        return placeDiagnostics(tr.state.doc.toString(), e.value);
      }
    }
    if (tr.docChanged) {
      return placed
        .map((p) => ({
          ...p,
          from: tr.changes.mapPos(p.from),
          to: tr.changes.mapPos(p.to, 1),
        }))
        .filter((p) => p.from < p.to);
    }
    return placed;
  },
  provide: (f) => EditorView.decorations.from(f, decorations),
});

// Build the tooltip body: one row per diagnostic. Each row carries a
// severity-coloured left accent, the message in the foreground ink, and the help
// (when present) on a dimmed line below — distinct from the squiggle classes so
// the tooltip text is never itself underlined.
export function diagnosticTooltipDom(hits: PlacedDiagnostic[]): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-diag-tooltip";
  for (const h of hits) {
    const row = document.createElement("div");
    row.className = `cm-diag-row cm-diag-row-${h.severity}`;

    const msg = document.createElement("div");
    msg.className = "cm-diag-msg";
    msg.textContent = h.message;
    row.appendChild(msg);

    if (h.help) {
      const help = document.createElement("div");
      help.className = "cm-diag-help";
      help.textContent = h.help;
      row.appendChild(help);
    }

    dom.appendChild(row);
  }
  return dom;
}

// Hover tooltip listing the diagnostics under the pointer, message plus help.
const diagnosticHover = hoverTooltip((view, pos) => {
  const hits = diagnosticsAt(view.state.field(diagnosticsField), pos);
  if (hits.length === 0) return null;
  return {
    pos: Math.min(...hits.map((h) => h.from)),
    end: Math.max(...hits.map((h) => h.to)),
    create: () => ({ dom: diagnosticTooltipDom(hits) }),
  };
});

// WKWebView (the macOS webview Tauri embeds) ignores the standard
// `text-decoration: underline wavy <color>` shorthand and needs the
// `-webkit-text-decoration` form, so set both. Without the prefixed property the
// squiggles render on web (Chromium/Firefox) but are invisible on desktop.
const wavy = (color: string) => ({
  textDecoration: `underline wavy ${color}`,
  WebkitTextDecoration: `underline wavy ${color}`,
});

const diagnosticTheme = EditorView.baseTheme({
  ".cm-diag-error": wavy("#e45649"),
  ".cm-diag-warning": wavy("#c18401"),
  ".cm-diag-info": wavy("#4078f2"),
});

// Re-skin the hover tooltip to the app's semantic tokens. CodeMirror's default
// `.cm-tooltip` is a fixed light-grey chip that ignores our CSS-var theme, so in
// the dark theme it rendered light `--fg` text on a light chip — invisible until
// selected (T7.27). An `EditorView.theme` (not baseTheme) outweighs CM's default,
// and the `var(...)` cascade resolves to the active theme; backgrounds/borders
// need no WKWebView prefixing. Mirrors the completion popup's chrome.
const diagnosticTooltipTheme = EditorView.theme({
  ".cm-tooltip.cm-tooltip-hover": {
    background: "var(--bg)",
    color: "var(--fg)",
    border: "1px solid var(--border)",
    borderRadius: "0.4rem",
    boxShadow: "0 6px 18px color-mix(in srgb, var(--fg) 18%, transparent)",
    overflow: "hidden",
  },
  ".cm-diag-tooltip": {
    padding: "5px 0",
    maxWidth: "340px",
    fontSize: "90%",
    fontFamily: "inherit",
  },
  ".cm-diag-row": {
    padding: "2px 10px 2px 9px",
    borderLeft: "3px solid transparent",
    lineHeight: "1.4",
  },
  ".cm-diag-row + .cm-diag-row": {
    marginTop: "3px",
    borderTop: "1px solid var(--border)",
    paddingTop: "5px",
  },
  ".cm-diag-row-error": { borderLeftColor: "var(--error)" },
  ".cm-diag-row-warning": { borderLeftColor: "var(--warning)" },
  ".cm-diag-row-info": { borderLeftColor: "var(--info)" },
  ".cm-diag-msg": { color: "var(--fg)" },
  ".cm-diag-help": {
    color: "var(--muted)",
    marginTop: "1px",
    fontSize: "92%",
  },
});

// Editor extension that underlines the core's diagnostics and shows them on hover.
export const diagnostics = [
  diagnosticsField,
  diagnosticHover,
  diagnosticTheme,
  diagnosticTooltipTheme,
];
