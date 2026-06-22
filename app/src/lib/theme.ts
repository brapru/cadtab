// App colour theme. "system" defers to the OS preference; the others force a
// mode. Cycling the toggle steps through them in this order.
export type Theme = "system" | "light" | "dark";

export const THEMES: Theme[] = ["system", "light", "dark"];

export function nextTheme(current: Theme): Theme {
  return THEMES[(THEMES.indexOf(current) + 1) % THEMES.length];
}

// A short glyph for the toggle button, per theme.
export function themeGlyph(theme: Theme): string {
  return theme === "light" ? "☀" : theme === "dark" ? "☾" : "◐";
}
