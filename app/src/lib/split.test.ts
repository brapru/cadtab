import { describe, it, expect } from "vitest";
import { clampSplit, splitFromPointer, MIN_SPLIT, MAX_SPLIT } from "./split";

describe("clampSplit", () => {
  it("passes through values inside the bounds", () => {
    expect(clampSplit(0.5)).toBe(0.5);
  });

  it("clamps to the min and max", () => {
    expect(clampSplit(-1)).toBe(MIN_SPLIT);
    expect(clampSplit(2)).toBe(MAX_SPLIT);
  });
});

describe("splitFromPointer", () => {
  it("computes the pointer's fraction across the container", () => {
    expect(splitFromPointer(50, { left: 0, width: 100 })).toBe(0.5);
    expect(splitFromPointer(30, { left: 0, width: 100 })).toBeCloseTo(0.3);
  });

  it("accounts for the container's left offset", () => {
    expect(splitFromPointer(70, { left: 20, width: 100 })).toBeCloseTo(0.5);
  });

  it("clamps extremes and falls back to even when unmeasured", () => {
    expect(splitFromPointer(0, { left: 0, width: 100 })).toBe(MIN_SPLIT);
    expect(splitFromPointer(1000, { left: 0, width: 100 })).toBe(MAX_SPLIT);
    expect(splitFromPointer(50, { left: 0, width: 0 })).toBe(0.5);
  });
});
