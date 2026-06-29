import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { CompletionContext } from "@codemirror/autocomplete";
import type { Completions } from "./types";
import {
  planCompletions,
  completionSource,
  completionsField,
  completionEnabledField,
  setCompletions,
  setCompletionEnabled,
  emptyCompletions,
} from "./completion";

// A vocabulary shaped like the core's output (one keyword per operand kind plus
// a couple of identifiers), so the planner sees every branch.
const vocab: Completions = {
  keywords: [
    { name: "title", operand: "string", values: [] },
    { name: "tempo", operand: "number", values: [] },
    { name: "instrument", operand: "values", values: ["banjo", "guitar"] },
    { name: "barnumbers", operand: "values", values: ["lines", "all", "off"] },
    { name: "score", operand: "none", values: [] },
  ],
  identifiers: ["forward_roll", "my_lick"],
};

const labels = (cands: { label: string }[]) => cands.map((c) => c.label);

describe("planCompletions", () => {
  it("offers keywords and identifiers in a general name position", () => {
    const plan = planCompletions("for", vocab);
    expect(plan.position).toBe("general");
    expect(plan.partial).toBe("for");
    expect(labels(plan.candidates)).toContain("title"); // keyword
    expect(labels(plan.candidates)).toContain("forward_roll"); // identifier
    // The planner returns the full set; CodeMirror filters by the partial.
    expect(plan.candidates).toHaveLength(
      vocab.keywords.length + vocab.identifiers.length,
    );
  });

  it("hints a value-set keyword's values in its operand slot", () => {
    const empty = planCompletions("instrument ", vocab);
    expect(empty.position).toBe("operand");
    expect(empty.partial).toBe("");
    expect(labels(empty.candidates)).toEqual(["banjo", "guitar"]);
    expect(empty.candidates.every((c) => c.kind === "value")).toBe(true);

    const partial = planCompletions("instrument ban", vocab);
    expect(partial.partial).toBe("ban");
    expect(labels(partial.candidates)).toEqual(["banjo", "guitar"]);
  });

  it("hints a quoted placeholder for a string operand", () => {
    const plan = planCompletions("title ", vocab);
    expect(plan.position).toBe("operand");
    expect(plan.candidates).toHaveLength(1);
    const [hint] = plan.candidates;
    expect(hint.kind).toBe("operand");
    expect(hint.label).toBe('"Title"');
    expect(hint.snippet).toBe('"${Title}"');
  });

  it("suppresses completion for a number operand", () => {
    const plan = planCompletions("tempo ", vocab);
    expect(plan.position).toBe("operand");
    expect(plan.candidates).toHaveLength(0);
  });

  it("suppresses completion in a structural keyword's slot", () => {
    // `score {` opens a block, not a value — offer nothing rather than names.
    expect(planCompletions("score ", vocab).candidates).toHaveLength(0);
  });

  it("falls through to identifiers once the line holds more than a directive", () => {
    const plan = planCompletions("score { 3:0 forward", vocab);
    expect(plan.position).toBe("general");
    expect(plan.partial).toBe("forward");
    expect(labels(plan.candidates)).toContain("forward_roll");
  });

  it("allows leading indentation before a directive slot", () => {
    expect(labels(planCompletions("  instrument b", vocab).candidates)).toEqual(
      ["banjo", "guitar"],
    );
    expect(labels(planCompletions("\tbarnumbers ", vocab).candidates)).toEqual([
      "lines",
      "all",
      "off",
    ]);
  });

  it("treats an unknown leading word as a general position", () => {
    // Not a keyword, so it's not an operand slot — offer names, partial empty.
    const plan = planCompletions("notakeyword ", vocab);
    expect(plan.position).toBe("general");
    expect(plan.candidates.length).toBeGreaterThan(0);
  });
});

describe("completionsField", () => {
  it("defaults to the empty vocabulary and updates on the effect", () => {
    let state = EditorState.create({ extensions: [completionsField] });
    expect(state.field(completionsField)).toBe(emptyCompletions);

    state = state.update({ effects: setCompletions.of(vocab) }).state;
    expect(state.field(completionsField)).toBe(vocab);
  });
});

describe("completionSource", () => {
  // A state carrying `doc` plus the vocabulary, for a constructed context.
  function stateWith(doc: string): EditorState {
    const base = EditorState.create({ doc, extensions: [completionsField] });
    return base.update({ effects: setCompletions.of(vocab) }).state;
  }

  it("stays quiet at an empty general position unless invoked explicitly", () => {
    const state = stateWith("");
    expect(completionSource(new CompletionContext(state, 0, false))).toBeNull();

    const explicit = completionSource(new CompletionContext(state, 0, true));
    expect(explicit).not.toBeNull();
    expect(explicit?.from).toBe(0);
    expect(explicit?.options.length).toBe(
      vocab.keywords.length + vocab.identifiers.length,
    );
  });

  it("opens a value hint as soon as the operand slot does, implicitly", () => {
    const state = stateWith("instrument ");
    const result = completionSource(
      new CompletionContext(state, state.doc.length, false),
    );
    expect(result).not.toBeNull();
    expect(result?.from).toBe(state.doc.length); // empty partial: replace nothing
    expect(result?.options.map((o) => o.label)).toEqual(["banjo", "guitar"]);
  });

  it("positions `from` at the start of a partially-typed value", () => {
    const state = stateWith("instrument ban");
    const result = completionSource(
      new CompletionContext(state, state.doc.length, false),
    );
    expect(result?.from).toBe(state.doc.length - 3); // before "ban"
  });

  it("offers a snippet for a string operand hint", () => {
    const state = stateWith("title ");
    const result = completionSource(
      new CompletionContext(state, state.doc.length, false),
    );
    expect(result?.options).toHaveLength(1);
    const [hint] = result!.options;
    expect(hint.label).toBe('"Title"');
    expect(hint.type).toBe("text");
    // snippetCompletion attaches an `apply` that inserts the placeholder.
    expect(typeof hint.apply).toBe("function");
  });

  it("returns null when there is nothing to offer", () => {
    const state = stateWith("tempo ");
    expect(
      completionSource(new CompletionContext(state, state.doc.length, true)),
    ).toBeNull();
  });

  it("offers nothing at all when the setting is off", () => {
    // Even an explicit invoke in an operand slot stays silent once disabled.
    const base = EditorState.create({
      doc: "instrument ",
      extensions: [completionsField, completionEnabledField],
    });
    const state = base.update({
      effects: [setCompletions.of(vocab), setCompletionEnabled.of(false)],
    }).state;
    expect(
      completionSource(new CompletionContext(state, state.doc.length, true)),
    ).toBeNull();
  });
});

describe("completionEnabledField", () => {
  it("defaults to enabled and follows the effect", () => {
    let state = EditorState.create({ extensions: [completionEnabledField] });
    expect(state.field(completionEnabledField)).toBe(true);

    state = state.update({ effects: setCompletionEnabled.of(false) }).state;
    expect(state.field(completionEnabledField)).toBe(false);
  });
});
