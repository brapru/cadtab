import { render } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import Tab from "./Tab.svelte";
import type { RenderTree } from "./types";

const tree: RenderTree = {
  meta: { width: 12, height: 4 },
  header: [],
  systems: [
    {
      bounds: { x: 0, y: 0, w: 12, h: 4 },
      prims: [],
      measures: [
        {
          bounds: { x: 0, y: 0, w: 12, h: 4 },
          prims: [
            { kind: "line", x1: 0, y1: 2, x2: 12, y2: 2, weight: 0.1 },
            {
              kind: "text",
              x: 1,
              y: 2,
              content: "0",
              role: "fretNumber",
              span: null,
            },
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
    expect(svg!.getAttribute("viewBox")).toBe("0 0 12 4");
  });

  it("paints line and text primitives", () => {
    const { container } = render(Tab, { props: { tree } });

    const line = container.querySelector("line");
    expect(line).not.toBeNull();
    expect(line!.getAttribute("x2")).toBe("12");
    expect(line!.getAttribute("stroke-width")).toBe("0.1");

    const text = container.querySelector("text");
    expect(text?.textContent).toBe("0");
    expect(text?.getAttribute("data-role")).toBe("fretNumber");
  });
});
