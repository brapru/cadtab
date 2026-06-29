import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import Tab from "./Tab.svelte";
import type { RenderTree } from "./types";

const tree: RenderTree = {
  meta: { width: 12, height: 8 },
  header: [
    {
      kind: "text",
      x: 6,
      y: 1,
      content: "Cripple Creek",
      role: "title",
      span: null,
    },
  ],
  systems: [
    {
      bounds: { x: 0, y: 0, w: 12, h: 8 },
      prims: [
        // String line (hairline) and a thick barline: lines differ by weight.
        { kind: "line", x1: 0, y1: 2, x2: 12, y2: 2, weight: 0.06 },
        { kind: "line", x1: 12, y1: 2, x2: 12, y2: 6, weight: 0.25 },
      ],
      measures: [
        {
          bounds: { x: 0, y: 0, w: 12, h: 8 },
          prims: [
            {
              kind: "text",
              x: 1,
              y: 2,
              content: "0",
              role: "fretNumber",
              span: { start: 0, end: 3 },
            },
            {
              kind: "text",
              x: 1,
              y: 7,
              content: "T",
              role: "finger",
              span: null,
            },
            // A tie arc: an open path that must not be filled.
            { kind: "path", cmds: "M1 2 Q2 1 3 2", span: null },
          ],
          span: null,
        },
      ],
    },
  ],
};

describe("Tab painter", () => {
  it("sets the svg viewBox from layout meta", () => {
    const { container } = render(Tab, { props: { tree } });
    const svg = container.querySelector("svg");
    expect(svg).not.toBeNull();
    expect(svg!.getAttribute("viewBox")).toBe("0 0 12 8");
  });

  // Line primitives carry their weight as stroke-width, with butt caps so thick
  // beams end exactly at the stems rather than overshooting with rounded ends.
  it("paints lines with their weight and butt caps", () => {
    const { container } = render(Tab, { props: { tree } });
    const lines = container.querySelectorAll("line");
    expect(lines).toHaveLength(2);
    expect(lines[0].getAttribute("stroke-width")).toBe("0.06");
    expect(lines[1].getAttribute("stroke-width")).toBe("0.25");
    expect(lines[0].getAttribute("stroke-linecap")).toBe("butt");
  });

  // Text primitives are tagged by role and sized per role.
  it("paints text with role-based size and metadata", () => {
    const { container } = render(Tab, { props: { tree } });
    const fret = container.querySelector('text[data-role="fretNumber"]');
    expect(fret?.textContent).toBe("0");
    expect(fret?.getAttribute("font-size")).toBe("1.3");

    const title = container.querySelector('text[data-role="title"]');
    expect(title?.textContent).toBe("Cripple Creek");
    expect(title?.getAttribute("font-size")).toBe("2.2");
    expect(title?.getAttribute("font-weight")).toBe("600");

    // Distinct roles get distinct sizes so the painter can differentiate them.
    expect(fret?.getAttribute("font-size")).not.toBe(
      title?.getAttribute("font-size"),
    );
  });

  it("paints def-gallery card text by role (bold heading, italic note)", () => {
    const galleryTree: RenderTree = {
      meta: { width: 12, height: 8 },
      header: [
        {
          kind: "text",
          x: 2,
          y: 1,
          content: "forward_roll(c)",
          role: "defHeading",
          span: null,
        },
        {
          kind: "text",
          x: 2,
          y: 2,
          content: "parameterized — no preview",
          role: "defNote",
          span: null,
        },
      ],
      systems: [],
    };
    const { container } = render(Tab, { props: { tree: galleryTree } });
    const heading = container.querySelector('text[data-role="defHeading"]');
    expect(heading?.textContent).toBe("forward_roll(c)");
    expect(heading?.getAttribute("font-weight")).toBe("700");
    const note = container.querySelector('text[data-role="defNote"]');
    expect(note?.textContent).toBe("parameterized — no preview");
    expect(note?.getAttribute("font-style")).toBe("italic");
  });

  // Path primitives stroke their geometry without a fill.
  it("paints paths as unfilled stroked curves", () => {
    const { container } = render(Tab, { props: { tree } });
    const path = container.querySelector("path");
    expect(path).not.toBeNull();
    expect(path!.getAttribute("d")).toBe("M1 2 Q2 1 3 2");
    expect(path!.getAttribute("fill")).toBe("none");
  });

  // Zoom multiplies the rendered width via a CSS token.
  it("reflects the zoom prop in the svg style", () => {
    const { container } = render(Tab, { props: { tree, zoom: 1.5 } });
    const svg = container.querySelector("svg");
    expect(svg!.style.getPropertyValue("--tab-zoom")).toBe("1.5");
  });

  it("defaults zoom to 1 when not provided", () => {
    const { container } = render(Tab, { props: { tree } });
    const svg = container.querySelector("svg");
    expect(svg!.style.getPropertyValue("--tab-zoom")).toBe("1");
  });

  // Render -> source: clicking a span-bearing primitive reports its span.
  it("reports a clicked primitive's span", async () => {
    const onPrimitiveClick = vi.fn();
    const { container } = render(Tab, { props: { tree, onPrimitiveClick } });
    const fret = container.querySelector('text[data-role="fretNumber"]')!;
    await fireEvent.click(fret);
    expect(onPrimitiveClick).toHaveBeenCalledWith({ start: 0, end: 3 });
  });

  it("activates a focused primitive via the keyboard", async () => {
    const onPrimitiveClick = vi.fn();
    const { container } = render(Tab, { props: { tree, onPrimitiveClick } });
    const fret = container.querySelector('text[data-role="fretNumber"]')!;
    await fireEvent.keyDown(fret, { key: "Enter" });
    expect(onPrimitiveClick).toHaveBeenCalledWith({ start: 0, end: 3 });
  });

  // Source -> render: a primitive whose span overlaps the active span is marked.
  it("marks primitives overlapping the active span", () => {
    const { container } = render(Tab, {
      props: { tree, activeSpan: { start: 1, end: 2 } },
    });
    const fret = container.querySelector('text[data-role="fretNumber"]');
    expect(fret?.classList.contains("active")).toBe(true);
    // The spanless title is never interactive.
    const title = container.querySelector('text[data-role="title"]');
    expect(title?.classList.contains("active")).toBe(false);
    expect(title?.classList.contains("clickable")).toBe(false);
  });
});
