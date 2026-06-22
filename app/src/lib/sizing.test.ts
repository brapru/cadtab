import { describe, it, expect } from "vitest";
import {
  layoutWidthForPx,
  clampZoom,
  PX_PER_UNIT,
  MIN_LAYOUT_WIDTH,
  MIN_ZOOM,
  MAX_ZOOM,
} from "./sizing";

describe("layoutWidthForPx", () => {
  it("scales pixels into logical units", () => {
    expect(layoutWidthForPx(1200)).toBe(1200 / PX_PER_UNIT);
    expect(layoutWidthForPx(1200, 12)).toBe(100);
  });

  it("never drops below the minimum layout width", () => {
    expect(layoutWidthForPx(0)).toBe(MIN_LAYOUT_WIDTH);
    expect(layoutWidthForPx(12)).toBe(MIN_LAYOUT_WIDTH);
  });
});

describe("clampZoom", () => {
  it("passes through values within range and clamps the extremes", () => {
    expect(clampZoom(1)).toBe(1);
    expect(clampZoom(100)).toBe(MAX_ZOOM);
    expect(clampZoom(0.01)).toBe(MIN_ZOOM);
  });
});
