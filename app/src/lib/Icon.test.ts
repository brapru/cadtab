import { render } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import Icon from "./Icon.svelte";

function glyph(container: HTMLElement) {
  return container.querySelector<HTMLElement>(".material-symbols-outlined")!;
}

describe("Icon", () => {
  it("renders the symbol ligature inside a Material Symbols span", () => {
    const { container } = render(Icon, { name: "close" });
    const el = glyph(container);
    expect(el.textContent).toBe("close");
    expect(el.classList.contains("material-symbols-outlined")).toBe(true);
    // translate=no keeps the browser from mangling the ligature text.
    expect(el.getAttribute("translate")).toBe("no");
  });

  it("is decorative by default — hidden from assistive tech, no role", () => {
    const { container } = render(Icon, { name: "add" });
    const el = glyph(container);
    expect(el.getAttribute("aria-hidden")).toBe("true");
    expect(el.getAttribute("role")).toBeNull();
    expect(el.getAttribute("aria-label")).toBeNull();
  });

  it("exposes itself as a labelled image when given a label", () => {
    const { container } = render(Icon, { name: "save", label: "Save score" });
    const el = glyph(container);
    expect(el.getAttribute("role")).toBe("img");
    expect(el.getAttribute("aria-label")).toBe("Save score");
    expect(el.getAttribute("aria-hidden")).toBeNull();
  });

  it("defaults to a 20px glyph and accepts a numeric size as pixels", () => {
    const def = glyph(render(Icon, { name: "add" }).container);
    expect(def.style.fontSize).toBe("20px");
    const sized = glyph(render(Icon, { name: "add", size: 32 }).container);
    expect(sized.style.fontSize).toBe("32px");
  });

  it("passes a string size through verbatim", () => {
    const { container } = render(Icon, { name: "add", size: "1.5em" });
    expect(glyph(container).style.fontSize).toBe("1.5em");
  });

  it("reflects fill and weight in the variable-font axes", () => {
    const def = glyph(render(Icon, { name: "star" }).container);
    expect(def.style.fontVariationSettings).toContain("'FILL' 0");
    expect(def.style.fontVariationSettings).toContain("'wght' 400");
    const filled = glyph(
      render(Icon, { name: "star", fill: true, weight: 600 }).container,
    );
    expect(filled.style.fontVariationSettings).toContain("'FILL' 1");
    expect(filled.style.fontVariationSettings).toContain("'wght' 600");
  });
});
