import type { Span } from "./types";

// Rust spans are UTF-8 byte offsets; CodeMirror positions are UTF-16 code-unit
// indices. They diverge wherever the source holds a multi-byte character (an
// accented title, a unicode string), so map byte offsets to char indices rather
// than trusting the two to coincide. Shared by highlighting and diagnostics.
export function byteToCharIndex(source: string): number[] {
  const map: number[] = [];
  let byte = 0;
  for (let i = 0; i < source.length; ) {
    const cp = source.codePointAt(i)!;
    const units = cp > 0xffff ? 2 : 1;
    const bytes = cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    for (let b = 0; b < bytes; b++) map[byte + b] = i;
    byte += bytes;
    i += units;
  }
  map[byte] = source.length;
  return map;
}

// The inverse of byteToCharIndex: map each UTF-16 code-unit offset to the byte
// offset where its character begins. Used to turn an editor cursor position back
// into the byte coordinates the render tree's spans speak.
export function charToByteIndex(source: string): number[] {
  const map: number[] = [];
  let byte = 0;
  for (let i = 0; i < source.length; ) {
    const cp = source.codePointAt(i)!;
    const units = cp > 0xffff ? 2 : 1;
    const bytes = cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    for (let u = 0; u < units; u++) map[i + u] = byte;
    byte += bytes;
    i += units;
  }
  map[source.length] = byte;
  return map;
}

export interface CharRange {
  from: number;
  to: number;
}

// Map a byte span to a CodeMirror char range via a byteToCharIndex map, or null
// when the span is empty or reaches outside the source.
export function spanToRange(map: number[], span: Span): CharRange | null {
  const from = map[span.start];
  const to = map[span.end];
  if (from === undefined || to === undefined || from >= to) return null;
  return { from, to };
}
