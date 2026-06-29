import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import {
  placeDiagnostics,
  diagnosticsAt,
  diagnosticTooltipDom,
  diagnosticsField,
  setDiagnostics,
  diagnosticCounts,
  diagnostics as diagnosticsExtension,
} from "./diagnostics";
import type { Diagnostic } from "./types";

function diag(
  severity: Diagnostic["severity"],
  start: number,
  end: number,
  message = "boom",
  help: string | null = null,
): Diagnostic {
  return { severity, span: { start, end }, message, help };
}

describe("diagnosticCounts", () => {
  it("tallies errors and warnings, ignoring info", () => {
    expect(
      diagnosticCounts([
        diag("error", 0, 1),
        diag("warning", 1, 2),
        diag("error", 2, 3),
        diag("info", 3, 4),
      ]),
    ).toEqual({ errors: 2, warnings: 1 });
  });

  it("is zero for no diagnostics", () => {
    expect(diagnosticCounts([])).toEqual({ errors: 0, warnings: 0 });
  });
});

describe("placeDiagnostics", () => {
  it("resolves byte spans and carries severity/message/help", () => {
    const placed = placeDiagnostics("score { 3:0 }", [
      diag("warning", 8, 11, "under-full", "needs more"),
    ]);
    expect(placed).toEqual([
      {
        from: 8,
        to: 11,
        severity: "warning",
        message: "under-full",
        help: "needs more",
      },
    ]);
  });

  it("drops out-of-range diagnostics (stale compile)", () => {
    // A non-empty span reaching past the current source is left over from a
    // longer text; it stays dropped (the fallback only rescues zero-width spans).
    expect(placeDiagnostics("abc", [diag("error", 2, 99)])).toEqual([]);
  });

  it("underlines a zero-width diagnostic on the character before the point", () => {
    // "expected `}` at end of input"-style errors point between characters; they
    // fall back to the preceding character so the squiggle is still visible.
    const eof = placeDiagnostics("abc", [diag("error", 3, 3, "expected `}`")]);
    expect(eof).toEqual([
      {
        from: 2,
        to: 3,
        severity: "error",
        message: "expected `}`",
        help: null,
      },
    ]);
    // A point at the very start underlines the first character instead.
    const start = placeDiagnostics("abc", [diag("error", 0, 0)]);
    expect(start[0]).toMatchObject({ from: 0, to: 1 });
    // Nothing to underline in an empty document → dropped.
    expect(placeDiagnostics("", [diag("error", 0, 0)])).toEqual([]);
  });
});

describe("diagnosticsAt", () => {
  const placed = placeDiagnostics("score { 3:0 }", [diag("warning", 8, 11)]);

  it("finds diagnostics covering a position", () => {
    expect(diagnosticsAt(placed, 9)).toHaveLength(1);
    expect(diagnosticsAt(placed, 8)).toHaveLength(1); // inclusive at the edge
    expect(diagnosticsAt(placed, 11)).toHaveLength(1);
  });

  it("returns none away from any diagnostic", () => {
    expect(diagnosticsAt(placed, 2)).toEqual([]);
  });
});

describe("diagnosticTooltipDom", () => {
  it("renders a severity-classed row per diagnostic, with message and dimmed help", () => {
    const dom = diagnosticTooltipDom([
      { from: 0, to: 5, severity: "error", message: "bad", help: "fix it" },
      { from: 6, to: 9, severity: "warning", message: "iffy", help: null },
    ]);
    const rows = dom.querySelectorAll(".cm-diag-row");
    expect(rows).toHaveLength(2);

    // First row: severity-keyed class, message + help on separate lines.
    expect(rows[0].className).toContain("cm-diag-row-error");
    expect(rows[0].querySelector(".cm-diag-msg")?.textContent).toBe("bad");
    expect(rows[0].querySelector(".cm-diag-help")?.textContent).toBe("fix it");

    // Second row: no help → no help line, just the message.
    expect(rows[1].className).toContain("cm-diag-row-warning");
    expect(rows[1].querySelector(".cm-diag-msg")?.textContent).toBe("iffy");
    expect(rows[1].querySelector(".cm-diag-help")).toBeNull();
  });
});

describe("diagnosticsField", () => {
  it("builds placed diagnostics from a setDiagnostics effect", () => {
    const state = EditorState.create({
      doc: "score { 3:0 }",
      extensions: [diagnosticsExtension],
    });
    const next = state.update({
      effects: setDiagnostics.of([diag("warning", 8, 11)]),
    }).state;
    expect(next.field(diagnosticsField)).toHaveLength(1);
  });

  it("remaps diagnostics through an edit", () => {
    const state = EditorState.create({
      doc: "score { 3:0 }",
      extensions: [diagnosticsExtension],
    });
    const withDiag = state.update({
      effects: setDiagnostics.of([diag("warning", 8, 11)]),
    }).state;
    // Insert two characters at the front; the squiggle shifts to follow its text.
    const edited = withDiag.update({
      changes: { from: 0, insert: "  " },
    }).state;
    const placed = edited.field(diagnosticsField);
    expect(placed).toHaveLength(1);
    expect(placed[0].from).toBe(10);
    expect(placed[0].to).toBe(13);
  });
});
