import { StateField, StateEffect } from "@codemirror/state";
import { Decoration, EditorView, hoverTooltip } from "@codemirror/view";
import type { DecorationSet } from "@codemirror/view";
import type { Diagnostic, Severity } from "./types";
import { byteToCharIndex, spanToRange } from "./spans";

// A diagnostic positioned in CodeMirror (UTF-16) coordinates, ready to underline
// and to surface in a hover tooltip.
export interface PlacedDiagnostic {
  from: number;
  to: number;
  severity: Severity;
  message: string;
  help: string | null;
}

// Resolve diagnostic byte spans against the source, dropping any that are empty
// or out of range (e.g. left over from a stale compile of longer text).
export function placeDiagnostics(
  source: string,
  diagnostics: Diagnostic[],
): PlacedDiagnostic[] {
  const map = byteToCharIndex(source);
  const placed: PlacedDiagnostic[] = [];
  for (const d of diagnostics) {
    const r = spanToRange(map, d.span);
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

// Build the tooltip body: one row per diagnostic, message with help appended.
export function diagnosticTooltipDom(hits: PlacedDiagnostic[]): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-diag-tooltip";
  for (const h of hits) {
    const row = document.createElement("div");
    row.className = `cm-diag-row cm-diag-${h.severity}`;
    row.textContent = h.help ? `${h.message} — ${h.help}` : h.message;
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
  ".cm-diag-tooltip": {
    padding: "3px 7px",
    maxWidth: "320px",
    fontSize: "90%",
  },
  ".cm-diag-row + .cm-diag-row": { marginTop: "3px" },
});

// Editor extension that underlines the core's diagnostics and shows them on hover.
export const diagnostics = [diagnosticsField, diagnosticHover, diagnosticTheme];
