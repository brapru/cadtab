import { foldService } from "@codemirror/language";

export type FoldRange = { from: number; to: number };

// Match the curly block that *opens* on the queried line, skipping braces that
// sit inside strings or comments (mirroring the core lexer: `"…"` strings, `//`
// line comments, `/* … */` block comments). Returns the inner range to hide
// (the `{…}` contents) when the block runs past the line, else null. When a line
// opens more than one block the outermost brace wins.
export function foldableRange(
  doc: string,
  lineFrom: number,
  lineTo: number,
): FoldRange | null {
  const stack: number[] = [];
  const pairs: FoldRange[] = [];
  const n = doc.length;
  let i = 0;
  while (i < n) {
    const c = doc[i];
    if (c === '"') {
      i++; // opening quote
      while (i < n && doc[i] !== '"' && doc[i] !== "\n") {
        i += doc[i] === "\\" ? 2 : 1; // escapes consume the next char
      }
      i++; // closing quote / newline / EOF
      continue;
    }
    if (c === "/" && doc[i + 1] === "/") {
      i += 2;
      while (i < n && doc[i] !== "\n") i++;
      continue;
    }
    if (c === "/" && doc[i + 1] === "*") {
      i += 2;
      while (i < n && !(doc[i] === "*" && doc[i + 1] === "/")) i++;
      i += 2; // past the closing `*/` (harmless if unterminated)
      continue;
    }
    if (c === "{") stack.push(i);
    else if (c === "}") {
      const open = stack.pop();
      if (open !== undefined) pairs.push({ from: open, to: i });
    }
    i++;
  }

  let best: FoldRange | null = null;
  for (const p of pairs) {
    // Opening brace on this line, closing brace on a later one.
    if (p.from >= lineFrom && p.from < lineTo && p.to > lineTo) {
      if (!best || p.from < best.from) best = p;
    }
  }
  if (!best) return null;

  // Hide from just after `{` up to the newline before the closing brace, so the
  // `}` stays on its own line (`score {…` / `}`) rather than collapsing to
  // `{…}`. Fall back to the brace itself when there is no such newline.
  const beforeClose = doc.lastIndexOf("\n", best.to);
  const to = beforeClose > best.from + 1 ? beforeClose : best.to;
  return { from: best.from + 1, to };
}

// CodeMirror fold source keyed to the DSL's brace structure.
export const foldByBraces = foldService.of((state, lineFrom, lineTo) =>
  foldableRange(state.doc.toString(), lineFrom, lineTo),
);
