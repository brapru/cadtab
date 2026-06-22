import { describe, it, expect } from "vitest";
import { byteToCharIndex, charToByteIndex, spanToRange } from "./spans";

describe("byteToCharIndex", () => {
  it("is identity for ascii", () => {
    const map = byteToCharIndex("abc");
    expect(map).toEqual([0, 1, 2, 3]);
  });

  it("collapses multi-byte characters to a single char index", () => {
    // "é" is two UTF-8 bytes (0,1) but one UTF-16 unit; the next char is index 1.
    const map = byteToCharIndex("é3");
    expect(map[0]).toBe(0);
    expect(map[1]).toBe(0);
    expect(map[2]).toBe(1); // start of "3"
    expect(map[3]).toBe(2); // end of source
  });
});

describe("charToByteIndex", () => {
  it("is identity for ascii", () => {
    expect(charToByteIndex("abc")).toEqual([0, 1, 2, 3]);
  });

  it("advances by utf-8 byte width past multi-byte characters", () => {
    // "é" occupies bytes 0..2, so the char at index 1 ("3") begins at byte 2.
    expect(charToByteIndex("é3")).toEqual([0, 2, 3]);
  });
});

describe("spanToRange", () => {
  const map = byteToCharIndex("é3"); // bytes: é=0..2, 3=2..3

  it("maps a byte span to a char range", () => {
    expect(spanToRange(map, { start: 2, end: 3 })).toEqual({ from: 1, to: 2 });
  });

  it("returns null for an empty span", () => {
    expect(spanToRange(map, { start: 2, end: 2 })).toBeNull();
  });

  it("returns null for a span reaching past the source", () => {
    expect(spanToRange(map, { start: 2, end: 99 })).toBeNull();
  });
});
