import { describe, it, expect } from "vitest";
import { foldableRange } from "./fold";

// Resolve the fold for the line containing `marker` (a unique substring),
// mirroring how CodeMirror calls the service with that line's bounds.
function foldForLineWith(
  doc: string,
  marker: string,
): { from: number; to: number } | null {
  const at = doc.indexOf(marker);
  const lineFrom = doc.lastIndexOf("\n", at) + 1;
  const nl = doc.indexOf("\n", at);
  const lineTo = nl === -1 ? doc.length : nl;
  return foldableRange(doc, lineFrom, lineTo);
}

describe("foldableRange", () => {
  it("folds the inner contents but keeps the closing brace on its own line", () => {
    const doc = "score {\n  3:0\n}";
    const range = foldForLineWith(doc, "score {")!;
    // From just after `{` to the newline before `}` (so the `}` line survives).
    expect(range).toEqual({
      from: doc.indexOf("{") + 1,
      to: doc.lastIndexOf("\n"),
    });
  });

  it("does not fold a block that opens and closes on one line", () => {
    const doc = "score { 3:0 }";
    expect(foldForLineWith(doc, "score")).toBeNull();
  });

  it("folds nested blocks independently", () => {
    const doc = "score {\n  measure {\n    3:0\n  }\n}";
    const outer = foldForLineWith(doc, "score {")!;
    const inner = foldForLineWith(doc, "measure {")!;
    expect(outer.from).toBe(doc.indexOf("{") + 1);
    // Each fold ends at the newline before its own closing brace's line.
    expect(outer.to).toBe(doc.lastIndexOf("\n"));
    expect(inner.from).toBe(doc.indexOf("measure {") + "measure ".length + 1);
    expect(inner.to).toBe(doc.lastIndexOf("\n", doc.indexOf("  }")));
  });

  it("ignores braces inside strings", () => {
    const doc = 'title "a { b"\nscore {\n  3:0\n}';
    // The brace in the title string must not pair with the score block.
    expect(foldForLineWith(doc, "title")).toBeNull();
    const range = foldForLineWith(doc, "score {")!;
    expect(range.from).toBe(doc.indexOf("score {") + "score ".length + 1);
    expect(range.to).toBe(doc.lastIndexOf("\n"));
  });

  it("ignores braces inside line and block comments", () => {
    const line = "// open {\nscore {\n  3:0\n}";
    expect(foldForLineWith(line, "// open")).toBeNull();
    expect(foldForLineWith(line, "score {")!.to).toBe(line.lastIndexOf("\n"));

    const block = "/* {\n still { comment */\nscore {\n  3:0\n}";
    expect(foldForLineWith(block, "score {")!.to).toBe(block.lastIndexOf("\n"));
  });

  it("returns null on a line with no opening brace", () => {
    const doc = "score {\n  3:0\n}";
    expect(foldForLineWith(doc, "3:0")).toBeNull();
  });
});
