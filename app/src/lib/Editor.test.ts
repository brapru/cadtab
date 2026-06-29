import { render, fireEvent } from "@testing-library/svelte";
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

  it("renders a line-number gutter, one number per line", async () => {
    const { container } = render(Editor, { props: { doc: "a\nb\nc" } });

    await vi.waitFor(() => {
      expect(container.querySelector(".cm-gutters")).not.toBeNull();
      const numbers = container.querySelectorAll(
        ".cm-lineNumbers .cm-gutterElement",
      );
      const labels = Array.from(numbers)
        .map((n) => n.textContent)
        .filter((t) => /^\d+$/.test(t ?? ""));
      // The numbered rows (the leading width-sizing spacer aside) count up.
      expect(labels.slice(-3)).toEqual(["1", "2", "3"]);
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

  it("inserts indentation on Tab instead of moving focus out", async () => {
    const onChange = vi.fn();
    const { container } = render(Editor, { props: { doc: "x", onChange } });

    let content!: Element;
    await vi.waitFor(() => {
      content = container.querySelector(".cm-content")!;
      expect(content).toBeTruthy();
    });

    // Cursor starts at the document head; Tab indents the line rather than
    // letting the browser advance focus.
    await fireEvent.keyDown(content, { key: "Tab" });
    await vi.waitFor(() => {
      const last = onChange.mock.calls.at(-1)?.[0] as string | undefined;
      expect(last).toBeDefined();
      expect(last).not.toBe("x");
      expect(/^\s/.test(last!)).toBe(true);
    });
  });

  it("selects the whole line on Cmd/Ctrl-L", async () => {
    const onCursor = vi.fn();
    const { container } = render(Editor, {
      props: { doc: "abc\ndef", onCursor },
    });

    let content!: Element;
    await vi.waitFor(() => {
      content = container.querySelector(".cm-content")!;
      expect(content).toBeTruthy();
    });

    // Cursor starts at line 1; Mod-L expands the selection to cover the whole
    // line including its trailing newline, so the head lands at pos 4.
    await fireEvent.keyDown(content, { key: "l", ctrlKey: true });
    await vi.waitFor(() => {
      expect(onCursor).toHaveBeenLastCalledWith(4);
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
