import { describe, it, expect } from "vitest";
import { nextTheme, themeIcon, THEMES } from "./theme";

describe("nextTheme", () => {
  it("cycles system -> light -> dark -> system", () => {
    expect(nextTheme("system")).toBe("light");
    expect(nextTheme("light")).toBe("dark");
    expect(nextTheme("dark")).toBe("system");
  });
});

describe("themeIcon", () => {
  it("gives a distinct icon name for every theme", () => {
    const icons = THEMES.map(themeIcon);
    expect(new Set(icons).size).toBe(THEMES.length);
  });
});
