import { describe, it, expect, vi, afterEach } from "vitest";
import { debounce } from "./debounce";

afterEach(() => vi.useRealTimers());

describe("debounce", () => {
  it("invokes once after the delay with the most recent arguments", () => {
    vi.useFakeTimers();
    const fn = vi.fn();
    const d = debounce(fn, 150);

    d("a");
    d("b");
    expect(fn).not.toHaveBeenCalled();

    vi.advanceTimersByTime(149);
    expect(fn).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenCalledTimes(1);
    expect(fn).toHaveBeenCalledWith("b");
  });
});
