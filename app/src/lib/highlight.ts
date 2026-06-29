import { StateField, StateEffect } from "@codemirror/state";
import { Decoration, EditorView } from "@codemirror/view";
import type { DecorationSet } from "@codemirror/view";
import type { Token } from "./types";
import { byteToCharIndex, spanToRange } from "./spans";

// A decoration range in CodeMirror (UTF-16) coordinates plus its CSS class.
export interface HighlightRange {
  from: number;
  to: number;
  cls: string;
}

// Convert classified tokens (byte spans) into CodeMirror decoration ranges,
// dropping anything empty or reaching outside the current source.
export function tokensToRanges(
  source: string,
  tokens: Token[],
): HighlightRange[] {
  const map = byteToCharIndex(source);
  const ranges: HighlightRange[] = [];
  for (const t of tokens) {
    const r = spanToRange(map, t.span);
    if (r) ranges.push({ ...r, cls: `cm-tok-${t.class}` });
  }
  return ranges;
}

function buildDecorations(source: string, tokens: Token[]): DecorationSet {
  const ranges = tokensToRanges(source, tokens).map((r) =>
    Decoration.mark({ class: r.cls }).range(r.from, r.to),
  );
  return Decoration.set(ranges, true);
}

// Effect carrying the latest token set for the current document.
export const setTokens = StateEffect.define<Token[]>();

// Holds the highlight decorations; remaps them through edits so colours track
// the text between recompiles, and rebuilds them whenever fresh tokens arrive.
export const tokenField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(setTokens)) {
        deco = buildDecorations(tr.state.doc.toString(), e.value);
      }
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

// Token-class colours, drawn from the muted two-tone palette tokens in app.css
// (T7.31) so highlighting re-themes with the rest of the UI. Keywords and
// operators share the desaturated-blue structure tone; numbers are warm tan;
// strings muted green; comments a gray italic. Idents and punctuation stay the
// default ink so the fretted positions and braces read plainly.
const tokenTheme = EditorView.baseTheme({
  ".cm-tok-keyword": { color: "var(--syn-structure)" },
  ".cm-tok-operator": { color: "var(--syn-structure)" },
  ".cm-tok-number": { color: "var(--syn-number)" },
  ".cm-tok-string": { color: "var(--syn-string)" },
  ".cm-tok-comment": { color: "var(--syn-comment)", fontStyle: "italic" },
});

// Editor extension that paints the core's classified tokens as highlighting.
export const syntaxHighlighting = [tokenField, tokenTheme];
