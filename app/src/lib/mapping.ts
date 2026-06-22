import type { RenderTree, Span } from "./types";

// Whether a span covers a byte position (end-inclusive, so a cursor resting just
// past the last character of a note still maps to it).
export function spanCoversByte(span: Span, byte: number): boolean {
  return byte >= span.start && byte <= span.end;
}

// Whether two spans overlap (half-open), used to light up every primitive that
// shares the source range under the cursor.
export function spansOverlap(a: Span, b: Span): boolean {
  return a.start < b.end && b.start < a.end;
}

// Every span-bearing primitive's span in the tree, in document order: header
// labels, system furniture, then each measure's primitives. Lines carry no span.
export function primitiveSpans(tree: RenderTree): Span[] {
  const spans: Span[] = [];
  const collect = (prims: { kind: string; span?: Span | null }[]) => {
    for (const p of prims) {
      if ((p.kind === "text" || p.kind === "path") && p.span)
        spans.push(p.span);
    }
  };
  collect(tree.header);
  for (const system of tree.systems) {
    collect(system.prims);
    for (const measure of system.measures) collect(measure.prims);
  }
  return spans;
}

// The narrowest primitive span covering a byte position, or null. Narrowest wins
// so a cursor inside a note selects that note rather than its enclosing measure.
export function narrowestSpanAt(tree: RenderTree, byte: number): Span | null {
  let best: Span | null = null;
  for (const span of primitiveSpans(tree)) {
    if (!spanCoversByte(span, byte)) continue;
    if (best === null || span.end - span.start < best.end - best.start) {
      best = span;
    }
  }
  return best;
}
