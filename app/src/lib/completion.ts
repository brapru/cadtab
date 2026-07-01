import { StateField, StateEffect, EditorSelection } from "@codemirror/state";
import {
  EditorView,
  ViewPlugin,
  Decoration,
  WidgetType,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";
import {
  autocompletion,
  acceptCompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import type { Completions } from "./types";

export { acceptCompletion };

// The empty vocabulary, used until the first compile pushes one.
export const emptyCompletions: Completions = { keywords: [], identifiers: [] };

// Effect + field carrying the current document's completion vocabulary, pushed
// from the app exactly like tokens/diagnostics (T7.24a). The completion source
// reads it synchronously, so completing never blocks on the backend.
export const setCompletions = StateEffect.define<Completions>();

export const completionsField = StateField.define<Completions>({
  create: () => emptyCompletions,
  update(vocab, tr) {
    for (const e of tr.effects) if (e.is(setCompletions)) return e.value;
    return vocab;
  },
});

// Whether autocomplete + inline hints are active (T7.24c). The source consults
// this so the toggle can silence completions without tearing the extension out
// of the editor; off, the popup never opens and Tab falls back to indenting.
export const setCompletionEnabled = StateEffect.define<boolean>();

export const completionEnabledField = StateField.define<boolean>({
  create: () => true,
  update(on, tr) {
    for (const e of tr.effects) if (e.is(setCompletionEnabled)) return e.value;
    return on;
  },
});

// A planned candidate, independent of CodeMirror so the logic stays unit-
// testable. `kind` picks the affordance. (The single quoted-placeholder hint for
// a string operand is not a popup candidate — it renders as inline ghost text;
// see `operandGhost` + `operandGhostPlugin`, T7.34g.)
export interface Candidate {
  label: string;
  kind: "keyword" | "value" | "identifier";
  detail?: string;
}

export interface CompletionPlan {
  // Whether the cursor sits in a directive's operand slot (offer its value set
  // or operand hint) or a general name position (keywords + identifiers). A
  // general popup only auto-opens once a word is typed; an operand hint opens as
  // soon as the slot does.
  position: "operand" | "general";
  // The partial word already typed that the candidates replace.
  partial: string;
  candidates: Candidate[];
}

// A directive's single operand slot: `<keyword><space><partial-to-eol>`, the
// partial a (possibly empty) bare word. Anything more on the line — a second
// token, a brace, a quote, punctuation — fails to match and falls through to
// general name completion (so `score { foo` completes `foo` as an identifier).
const OPERAND_SLOT = /^\s*([a-z]+)[ \t]+([A-Za-z_]\w*|)$/;
// The bare word immediately before the cursor, for general name completion.
const TRAILING_WORD = /([A-Za-z_]\w*)?$/;

// The placeholder shown inside the quotes for a string directive
// (`title "Title"`), keyed off the keyword so the hint reads naturally.
const STRING_PLACEHOLDER: Record<string, string> = {
  title: "Title",
  composer: "Composer",
  capo: "Capo",
  import: "file.ctab",
};

// The completion candidates for a cursor, given the text on its line up to the
// cursor and the document's vocabulary. Pure: the CodeMirror adapter turns the
// plan into a positioned result.
export function planCompletions(
  lineBefore: string,
  vocab: Completions,
): CompletionPlan {
  const slot = OPERAND_SLOT.exec(lineBefore);
  if (slot) {
    const kw = vocab.keywords.find((k) => k.name === slot[1]);
    const partial = slot[2];
    if (kw && kw.operand === "values") {
      return {
        position: "operand",
        partial,
        candidates: kw.values.map((v) => ({ label: v, kind: "value" })),
      };
    }
    if (kw && kw.operand === "string") {
      // The quoted-placeholder hint is inline ghost text (operandGhost), not a
      // popup candidate (T7.34g) — so the popup stays out of the way here.
      return { position: "operand", partial, candidates: [] };
    }
    // A number operand (`tempo 120`) or a structural keyword (`score {`) has no
    // list to offer; suppress completion rather than mis-offer names.
    if (kw) return { position: "operand", partial, candidates: [] };
  }

  const word = TRAILING_WORD.exec(lineBefore)?.[1] ?? "";
  const candidates: Candidate[] = [
    ...vocab.keywords.map(
      (k): Candidate => ({ label: k.name, kind: "keyword" }),
    ),
    ...vocab.identifiers.map(
      (id): Candidate => ({ label: id, kind: "identifier" }),
    ),
  ];
  return { position: "general", partial: word, candidates };
}

// The inline ghost hint for a string-operand slot: the placeholder to show (and,
// on Tab, insert) once `<keyword><space>` is typed with nothing after it — e.g.
// `title ` → `Title` (rendered `"Title"`). Null anywhere else: a partial already
// typed, a non-string operand, or a non-keyword. Pure, so it unit-tests without
// CodeMirror; the ViewPlugin and the accept command both consult it.
export function operandGhost(
  lineBefore: string,
  vocab: Completions,
): string | null {
  const slot = OPERAND_SLOT.exec(lineBefore);
  if (!slot) return null;
  const kw = vocab.keywords.find((k) => k.name === slot[1]);
  const partial = slot[2];
  if (kw && kw.operand === "string" && partial === "") {
    return STRING_PLACEHOLDER[kw.name] ?? "value";
  }
  return null;
}

// Map a neutral candidate to a CodeMirror completion, typed by kind so the popup
// shows a sensible icon.
function toCompletion(c: Candidate): Completion {
  const type =
    c.kind === "keyword" ? "keyword" : c.kind === "value" ? "enum" : "function";
  return { label: c.label, type, detail: c.detail };
}

// The completion source: read the pushed vocabulary, plan candidates for the
// cursor's line, and position the result. Returns null (no popup) when there is
// nothing to offer, or — for an unprompted general position with no word yet —
// to stay out of the way until the user types or invokes completion explicitly.
export function completionSource(
  context: CompletionContext,
): CompletionResult | null {
  const enabled = context.state.field(completionEnabledField, false) ?? true;
  if (!enabled) return null;
  const vocab =
    context.state.field(completionsField, false) ?? emptyCompletions;
  const line = context.state.doc.lineAt(context.pos);
  const before = line.text.slice(0, context.pos - line.from);
  const plan = planCompletions(before, vocab);
  if (plan.candidates.length === 0) return null;
  if (!context.explicit && plan.position === "general" && plan.partial === "") {
    return null;
  }
  return {
    from: context.pos - plan.partial.length,
    options: plan.candidates.map(toCompletion),
    validFor: /^[\w]*$/,
  };
}

// Theme the completion popup to the app's semantic tokens so it matches the
// other elevated popups (export/context menus) on every theme, rather than
// CodeMirror's default chrome: the elevated --bg-chrome surface lifted by the
// shared --shadow-popup (T7.34d). The tooltip inherits the `--*` cascade, so
// `var(...)` resolves to the active theme; backgrounds/borders need no WKWebView
// prefixing.
const completionTheme = EditorView.theme({
  ".cm-tooltip.cm-tooltip-autocomplete": {
    background: "var(--bg-chrome)",
    border: "1px solid var(--border)",
    borderRadius: "0.4rem",
    boxShadow: "var(--shadow-popup)",
    overflow: "hidden",
  },
  ".cm-tooltip-autocomplete > ul": {
    fontFamily: "inherit",
    maxHeight: "16em",
  },
  ".cm-tooltip-autocomplete > ul > li": {
    padding: "0.18rem 0.5rem",
    color: "var(--fg)",
    lineHeight: "1.5",
  },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
    background: "color-mix(in srgb, var(--accent) 22%, transparent)",
    color: "var(--fg)",
  },
  // The matched substring of the typed prefix: accent, not CM's underline.
  ".cm-completionMatchedText": {
    color: "var(--accent)",
    fontWeight: "600",
    textDecoration: "none",
  },
  // The operand-hint detail (`title operand`) reads muted and secondary.
  ".cm-completionDetail": {
    color: "var(--muted)",
    fontStyle: "italic",
  },
  ".cm-completionIcon": {
    color: "var(--muted)",
    opacity: "0.8",
    marginRight: "0.4rem",
  },
  // Inline operand ghost text (T7.34g): a dimmed placeholder after the caret,
  // muted and faint so it reads as a transient hint, not real content.
  ".cm-operand-ghost": {
    color: "var(--muted)",
    opacity: "0.65",
  },
});

// The widget drawn after the caret for an operand ghost hint (e.g. `"Title"`).
class GhostHintWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }
  eq(other: GhostHintWidget) {
    return other.text === this.text;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = "cm-operand-ghost";
    span.textContent = this.text;
    return span;
  }
  // Purely decorative — never intercept the editor's own events.
  ignoreEvent() {
    return true;
  }
}

// Compute the ghost-hint decoration set for the current cursor: a single widget
// after the caret when the caret is a bare cursor at the end of an empty string-
// operand slot and completions are enabled. Empty otherwise.
function ghostDecorations(view: EditorView): DecorationSet {
  const enabled = view.state.field(completionEnabledField, false) ?? true;
  if (!enabled) return Decoration.none;
  const sel = view.state.selection.main;
  if (!sel.empty) return Decoration.none;
  const line = view.state.doc.lineAt(sel.head);
  // Only at the line's end — a hint mid-line would collide with real text.
  if (sel.head !== line.to) return Decoration.none;
  const before = line.text.slice(0, sel.head - line.from);
  const vocab = view.state.field(completionsField, false) ?? emptyCompletions;
  const placeholder = operandGhost(before, vocab);
  if (!placeholder) return Decoration.none;
  const widget = Decoration.widget({
    widget: new GhostHintWidget(`"${placeholder}"`),
    side: 1,
  });
  return Decoration.set([widget.range(sel.head)]);
}

// The ViewPlugin that keeps the ghost decoration in sync with the cursor. The
// check is a cheap single-line scan, so it just recomputes on every update.
export const operandGhostPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = ghostDecorations(view);
    }
    update(u: ViewUpdate) {
      this.decorations = ghostDecorations(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);

// Tab command: if an operand ghost is showing, insert its quoted placeholder
// with the placeholder text selected (so the user overtypes it) and consume the
// key; otherwise return false so Tab falls through (to `acceptCompletion`, then
// indentation). Mirrors the old snippet-insert, now driven by the ghost.
export function acceptOperandGhost(view: EditorView): boolean {
  const enabled = view.state.field(completionEnabledField, false) ?? true;
  if (!enabled) return false;
  const sel = view.state.selection.main;
  if (!sel.empty) return false;
  const line = view.state.doc.lineAt(sel.head);
  if (sel.head !== line.to) return false;
  const before = line.text.slice(0, sel.head - line.from);
  const vocab = view.state.field(completionsField, false) ?? emptyCompletions;
  const placeholder = operandGhost(before, vocab);
  if (!placeholder) return false;
  const from = sel.head;
  const inner = from + 1; // inside the opening quote
  view.dispatch({
    changes: { from, insert: `"${placeholder}"` },
    selection: EditorSelection.range(inner, inner + placeholder.length),
    scrollIntoView: true,
    userEvent: "input.complete",
  });
  return true;
}

// The editor extension: the vocabulary field plus autocompletion driven solely
// by our core-sourced candidates. Tab-to-accept is wired in the editor's keymap
// (before `indentWithTab`) via the re-exported `acceptCompletion`.
export const completion = [
  completionsField,
  completionEnabledField,
  completionTheme,
  operandGhostPlugin,
  autocompletion({
    override: [completionSource],
    activateOnTyping: true,
  }),
];
