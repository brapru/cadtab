import { render, screen } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import type { CompileResult } from "./lib/types";

const fake: CompileResult = {
  renderTree: {
    meta: { width: 12, height: 4 },
    header: [],
    systems: [
      {
        bounds: { x: 0, y: 0, w: 12, h: 4 },
        measures: [
          {
            bounds: { x: 0, y: 0, w: 12, h: 4 },
            prims: [
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
  },
  diagnostics: [],
  tokens: [],
};

vi.mock("./lib/wasm", () => ({
  compile: vi.fn(async () => fake),
}));

import App from "./App.svelte";

describe("App", () => {
  it("renders the title heading", () => {
    render(App);
    expect(screen.getByRole("heading", { name: "cadtab" })).toBeInTheDocument();
  });

  it("renders the compiled tab via the wasm backend", async () => {
    const { container } = render(App);
    await vi.waitFor(() => {
      expect(container.querySelector("svg.tab")).not.toBeNull();
    });
    expect(container.querySelector("text")?.textContent).toBe("0");
  });
});
