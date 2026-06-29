import { StateField, StateEffect } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import {
  autocompletion,
  acceptCompletion,
  snippetCompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import type { Completions, KeywordInfo } from "./types";

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
// testable. `kind` picks the affordance; `snippet`, when set, is a snippet
// template inserted instead of the bare label (operand hints insert a
// placeholder the user then overtypes).
export interface Candidate {
  label: string;
  kind: "keyword" | "value" | "identifier" | "operand";
  detail?: string;
  snippet?: string;
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

// The placeholder shown inside the inserted quotes for a string directive
// (`title "Title"`), keyed off the keyword so the hint reads naturally.
const STRING_PLACEHOLDER: Record<string, string> = {
  title: "Title",
  composer: "Composer",
  capo: "Capo",
  import: "file.ctab",
};

function operandHint(kw: KeywordInfo): Candidate {
  const placeholder = STRING_PLACEHOLDER[kw.name] ?? "value";
  return {
    label: `"${placeholder}"`,
    kind: "operand",
    detail: `${kw.name} operand`,
    // A snippet field (`${Title}`) so the inserted placeholder lands selected.
    snippet: `"\${${placeholder}}"`,
  };
}

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
      return { position: "operand", partial, candidates: [operandHint(kw)] };
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

// Map a neutral candidate to a CodeMirror completion. Operand hints insert a
// snippet (placeholder selected); the rest insert their label, typed by kind so
// the popup shows a sensible icon.
function toCompletion(c: Candidate): Completion {
  if (c.kind === "operand" && c.snippet) {
    return snippetCompletion(c.snippet, {
      label: c.label,
      type: "text",
      detail: c.detail,
    });
  }
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
// editor surface (and the topbar menu) on every theme, rather than CodeMirror's
// default chrome. The tooltip inherits the `--*` cascade, so `var(...)` resolves
// to the active theme; backgrounds/borders need no WKWebView prefixing.
const completionTheme = EditorView.theme({
  ".cm-tooltip.cm-tooltip-autocomplete": {
    background: "var(--bg)",
    border: "1px solid var(--border)",
    borderRadius: "0.4rem",
    boxShadow: "0 6px 18px color-mix(in srgb, var(--fg) 18%, transparent)",
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
});

// The editor extension: the vocabulary field plus autocompletion driven solely
// by our core-sourced candidates. Tab-to-accept is wired in the editor's keymap
// (before `indentWithTab`) via the re-exported `acceptCompletion`.
export const completion = [
  completionsField,
  completionEnabledField,
  completionTheme,
  autocompletion({
    override: [completionSource],
    activateOnTyping: true,
  }),
];
