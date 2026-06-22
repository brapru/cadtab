import { render } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import Editor from "./Editor.svelte";
import type { Token } from "./types";

describe("Editor highlighting", () => {
  it("decorates tokens pushed in via the tokens prop", async () => {
    const tokens: Token[] = [{ class: "keyword", span: { start: 0, end: 5 } }];
    const { container } = render(Editor, {
      props: { doc: "score", tokens },
    });

    await vi.waitFor(() => {
      const mark = container.querySelector(".cm-tok-keyword");
      expect(mark).not.toBeNull();
      expect(mark?.textContent).toBe("score");
    });
  });

  it("renders its own caret and active-line layers", async () => {
    const { container } = render(Editor, { props: { doc: "score" } });

    await vi.waitFor(() => {
      // drawSelection() draws CM's caret/selection layer; highlightActiveLine()
      // marks the line holding the cursor.
      expect(container.querySelector(".cm-cursorLayer")).not.toBeNull();
      expect(container.querySelector(".cm-activeLine")).not.toBeNull();
    });
  });
});
