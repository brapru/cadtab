import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import {
  tokensToRanges,
  tokenField,
  setTokens,
  syntaxHighlighting,
} from "./highlight";
import type { Token } from "./types";

function tok(cls: Token["class"], start: number, end: number): Token {
  return { class: cls, span: { start, end } };
}

describe("tokensToRanges", () => {
  it("maps ascii byte spans straight to char ranges with prefixed classes", () => {
    const source = "score { 3:0 }";
    const ranges = tokensToRanges(source, [
      tok("keyword", 0, 5),
      tok("number", 8, 9),
      tok("operator", 9, 10),
    ]);
    expect(ranges).toEqual([
      { from: 0, to: 5, cls: "cm-tok-keyword" },
      { from: 8, to: 9, cls: "cm-tok-number" },
      { from: 9, to: 10, cls: "cm-tok-operator" },
    ]);
  });

  it("converts multi-byte byte offsets to UTF-16 indices", () => {
    // "é" is two UTF-8 bytes but one UTF-16 unit, so a token after it sits at a
    // smaller char index than its byte offset.
    const source = "é3";
    const ranges = tokensToRanges(source, [tok("number", 2, 3)]);
    expect(ranges).toEqual([{ from: 1, to: 2, cls: "cm-tok-number" }]);
  });

  it("drops empty and out-of-range spans", () => {
    const source = "abc";
    const ranges = tokensToRanges(source, [
      tok("ident", 1, 1), // empty
      tok("number", 2, 99), // end past the source
    ]);
    expect(ranges).toEqual([]);
  });
});

describe("tokenField", () => {
  it("builds decorations from a setTokens effect", () => {
    const state = EditorState.create({
      doc: "score",
      extensions: [syntaxHighlighting],
    });
    const next = state.update({
      effects: setTokens.of([tok("keyword", 0, 5)]),
    }).state;
    expect(next.field(tokenField).size).toBe(1);
  });

  it("remaps existing decorations through an edit", () => {
    const state = EditorState.create({
      doc: "score",
      extensions: [syntaxHighlighting],
    });
    const withTokens = state.update({
      effects: setTokens.of([tok("keyword", 0, 5)]),
    }).state;
    // Insert two characters at the front; the decoration shifts but survives.
    const edited = withTokens.update({
      changes: { from: 0, insert: "  " },
    }).state;
    const deco = edited.field(tokenField);
    expect(deco.size).toBe(1);
    deco.between(0, edited.doc.length, (from, to) => {
      expect(from).toBe(2);
      expect(to).toBe(7);
    });
  });
});
