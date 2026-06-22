import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import {
  placeDiagnostics,
  diagnosticsAt,
  diagnosticTooltipDom,
  diagnosticsField,
  setDiagnostics,
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

  it("drops empty and out-of-range diagnostics", () => {
    const placed = placeDiagnostics("abc", [
      diag("error", 1, 1),
      diag("error", 2, 99),
    ]);
    expect(placed).toEqual([]);
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
  it("renders a severity-classed row per diagnostic, appending help", () => {
    const dom = diagnosticTooltipDom([
      { from: 0, to: 5, severity: "error", message: "bad", help: "fix it" },
      { from: 6, to: 9, severity: "warning", message: "iffy", help: null },
    ]);
    const rows = dom.querySelectorAll(".cm-diag-row");
    expect(rows).toHaveLength(2);
    expect(rows[0].className).toContain("cm-diag-error");
    expect(rows[0].textContent).toBe("bad — fix it");
    expect(rows[1].className).toContain("cm-diag-warning");
    expect(rows[1].textContent).toBe("iffy");
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
