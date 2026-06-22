import { describe, it, expect } from "vitest";
import { nextTheme, themeGlyph, THEMES } from "./theme";

describe("nextTheme", () => {
  it("cycles system -> light -> dark -> system", () => {
    expect(nextTheme("system")).toBe("light");
    expect(nextTheme("light")).toBe("dark");
    expect(nextTheme("dark")).toBe("system");
  });
});

describe("themeGlyph", () => {
  it("gives a distinct glyph for every theme", () => {
    const glyphs = THEMES.map(themeGlyph);
    expect(new Set(glyphs).size).toBe(THEMES.length);
  });
});
