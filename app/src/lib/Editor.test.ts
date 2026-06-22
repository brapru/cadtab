import { render } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import Editor from "./Editor.svelte";
import type { Token, Diagnostic } from "./types";

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

  it("underlines diagnostics pushed in via the diagnostics prop", async () => {
    const diagnostics: Diagnostic[] = [
      {
        severity: "error",
        span: { start: 0, end: 5 },
        message: "bad",
        help: null,
      },
    ];
    const { container } = render(Editor, {
      props: { doc: "score", diagnostics },
    });

    await vi.waitFor(() => {
      const squiggle = container.querySelector(".cm-diag-error");
      expect(squiggle).not.toBeNull();
      expect(squiggle?.textContent).toBe("score");
    });
  });

  it("applies an external selection and reports the cursor head", async () => {
    const onCursor = vi.fn();
    render(Editor, {
      props: { doc: "score { 3:0 }", selection: { from: 2, to: 4 }, onCursor },
    });

    // The selection effect moves the caret to the range head, which the update
    // listener reports back through onCursor.
    await vi.waitFor(() => {
      expect(onCursor).toHaveBeenCalledWith(4);
    });
  });
});
