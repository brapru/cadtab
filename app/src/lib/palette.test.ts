import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// app.css carries the semantic palette across four theme blocks (light default
// + prefers-color-scheme:dark + forced light + forced dark). They must stay in
// sync — a token defined in one block but missing in another silently breaks
// theming for that mode. These tests guard the T7.34a elevation foundation.
// vitest runs with cwd = app/ (npm --prefix app run test).
const css = readFileSync(resolve(process.cwd(), "src/app.css"), "utf8");

// Each anchor opens a flat declaration block (no nested braces); slice from its
// `{` to the next `}` to read just that theme's declarations.
const BLOCKS: Record<string, string> = {
  "light default": ":root {",
  "prefers dark": ':root:not([data-theme="light"]) {',
  "forced light": ':root[data-theme="light"] {',
  "forced dark": ':root[data-theme="dark"] {',
};

function block(anchor: string): string {
  const start = css.indexOf(anchor);
  if (start === -1) throw new Error(`block not found: ${anchor}`);
  const open = start + anchor.length;
  const close = css.indexOf("}", open);
  return css.slice(open, close);
}

const ELEVATION = ["--bg-chrome", "--bg-panel", "--bg-editor"];

describe("elevation tokens", () => {
  for (const [name, anchor] of Object.entries(BLOCKS)) {
    it(`defines the full elevation stack in the ${name} block`, () => {
      const body = block(anchor);
      for (const token of ELEVATION) {
        expect(body).toContain(`${token}:`);
      }
    });
  }

  it("steps each dark block chrome -> panel -> editor by darkening", () => {
    // The dark grounds darken toward the editing surface: chrome is the
    // lightest step, the editor the deepest.
    const lum = (hex: string) => parseInt(hex.slice(1, 3), 16);
    for (const name of ["prefers dark", "forced dark"] as const) {
      const body = block(BLOCKS[name]);
      const hex = (token: string) =>
        body.match(new RegExp(`${token}:\\s*(#[0-9a-fA-F]{6})`))![1];
      const chrome = lum(hex("--bg-chrome"));
      const panel = lum(hex("--bg-panel"));
      const editor = lum(hex("--bg-editor"));
      expect(chrome).toBeGreaterThan(panel);
      expect(panel).toBeGreaterThan(editor);
    }
  });

  it("keeps editor ink offset, never pure white, on the dark ground", () => {
    for (const name of ["prefers dark", "forced dark"] as const) {
      const fg = block(BLOCKS[name]).match(/--fg:\s*(#[0-9a-fA-F]{6})/)![1];
      expect(fg.toLowerCase()).not.toBe("#ffffff");
    }
  });
});
