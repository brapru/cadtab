import "@testing-library/jest-dom/vitest";

// jsdom has no layout engine, so the Range geometry CodeMirror uses to position
// its caret/selection layers is missing and `getClientRects` throws. Stub it
// (and the bounding box) with empty geometry so measure cycles run quietly.
const emptyRect: DOMRect = {
  x: 0,
  y: 0,
  top: 0,
  left: 0,
  right: 0,
  bottom: 0,
  width: 0,
  height: 0,
  toJSON: () => ({}),
};

Range.prototype.getClientRects = () =>
  ({
    length: 0,
    item: () => null,
    [Symbol.iterator]: function* () {},
  }) as unknown as DOMRectList;

Range.prototype.getBoundingClientRect = () => emptyRect;

// jsdom has no ResizeObserver, which Svelte's bind:clientWidth relies on. A
// no-op stub lets components mount; element widths just stay 0 in tests.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??=
  ResizeObserverStub as unknown as typeof ResizeObserver;
