// App colour theme. "system" defers to the OS preference; the others force a
// mode. Cycling the toggle steps through them in this order.
export type Theme = "system" | "light" | "dark";

export const THEMES: Theme[] = ["system", "light", "dark"];

export function nextTheme(current: Theme): Theme {
  return THEMES[(THEMES.indexOf(current) + 1) % THEMES.length];
}

// The Material Symbols icon for the toggle button, per theme.
export function themeIcon(theme: Theme): string {
  return theme === "light"
    ? "light_mode"
    : theme === "dark"
      ? "dark_mode"
      : "brightness_auto";
}
