<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { EditorState, EditorSelection } from "@codemirror/state";
  import {
    EditorView,
    keymap,
    drawSelection,
    dropCursor,
    highlightActiveLine,
    highlightActiveLineGutter,
    lineNumbers,
  } from "@codemirror/view";
  import {
    defaultKeymap,
    history,
    historyKeymap,
    indentWithTab,
    selectLine,
  } from "@codemirror/commands";
  import { codeFolding, foldGutter, foldKeymap } from "@codemirror/language";
  import { syntaxHighlighting, setTokens } from "./highlight";
  import { foldByBraces } from "./fold";
  import {
    diagnostics as diagnosticsExtension,
    setDiagnostics,
  } from "./diagnostics";
  import {
    completion as completionExtension,
    setCompletions,
    setCompletionEnabled,
    acceptCompletion,
    acceptOperandGhost,
    emptyCompletions,
  } from "./completion";
  import type { Token, Diagnostic, Completions } from "./types";

  let {
    doc = "",
    onChange,
    onCursor,
    onFocus,
    tokens = [],
    diagnostics = [],
    completions = emptyCompletions,
    autocomplete = true,
    selection = null,
    loadRequest = null,
    formatRequest = null,
    zoom = 1,
  }: {
    doc?: string;
    onChange?: (value: string) => void;
    onCursor?: (pos: number) => void;
    onFocus?: () => void;
    tokens?: Token[];
    diagnostics?: Diagnostic[];
    completions?: Completions;
    autocomplete?: boolean;
    selection?: { from: number; to: number } | null;
    loadRequest?: { content: string; token: number } | null;
    formatRequest?: { content: string; token: number } | null;
    zoom?: number;
  } = $props();

  let container: HTMLDivElement;
  let view = $state<EditorView | undefined>();
  // Highest load token applied, so a re-render doesn't re-replace the document.
  let lastLoadToken = -1;
  // Highest format token applied, so a re-render doesn't re-apply a format.
  let lastFormatToken = -1;

  // Build a fresh editor state for `text`. Loading a document rebuilds the state
  // (rather than editing the current one) so the loaded file becomes the undo
  // baseline — there is no "undo back to the previous document".
  function buildState(text: string): EditorState {
    return EditorState.create({
      doc: text,
      extensions: [
        history(),
        // A line-number gutter (with the active line's number highlighted to
        // match the active-line row), and a fold gutter keyed to the DSL's brace
        // structure: a down chevron on `{`-opening lines collapses the block.
        lineNumbers(),
        highlightActiveLineGutter(),
        codeFolding(),
        foldByBraces,
        foldGutter({
          markerDOM(open) {
            const el = document.createElement("span");
            el.className =
              "material-symbols-outlined cm-foldMarker" +
              (open ? "" : " cm-foldMarker-closed");
            // Down chevron when expanded; a side arrow (coloured) when folded.
            el.textContent = open ? "keyboard_arrow_down" : "chevron_right";
            return el;
          },
        }),
        // CM's own caret + selection layer (the native one only shows on focus
        // and can't be themed), a drop caret, and an active-line highlight.
        drawSelection(),
        dropCursor(),
        highlightActiveLine(),
        // Tab accepts an open completion (T7.24), else an inline operand ghost
        // hint (T7.34g), else inserts indentation rather than moving focus out;
        // Cmd/Ctrl-L selects the whole line (Mod maps to Cmd on macOS, Ctrl
        // elsewhere). Each accept binding precedes `indentWithTab` and is a
        // no-op (falls through) when its affordance isn't showing.
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...foldKeymap,
          { key: "Tab", run: acceptCompletion },
          { key: "Tab", run: acceptOperandGhost },
          indentWithTab,
          { key: "Mod-l", run: selectLine },
        ]),
        // Bind the editor surface to the app's semantic theme tokens so it
        // re-themes (background, caret, selection) with everything else.
        EditorView.theme({
          "&": { backgroundColor: "var(--bg-editor)", color: "var(--fg)" },
          ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--fg)" },
          // The active line wears a faint accent tick at its left edge rather
          // than a full-width fill (T7.31), so the row reads without washing out
          // the caret, selection, or token colours sitting on it.
          ".cm-activeLine": {
            backgroundColor: "transparent",
            boxShadow:
              "inset 2px 0 0 color-mix(in srgb, var(--accent) 55%, transparent)",
          },
          // Line-number gutter: themed muted numbers on the editor background,
          // with a divider rule separating it from the code text.
          ".cm-gutters": {
            backgroundColor: "var(--bg-editor)",
            color: "var(--muted)",
            borderRight: "1px solid var(--border)",
          },
          // Gutter spacing reads symmetric: more outer-left on the (right-
          // aligned) numbers, a 6+6px gap to the fold arrows, and less outer-
          // right on the arrows by the divider, so both edges look evenly inset.
          ".cm-lineNumbers .cm-gutterElement": {
            padding: "0 4px 0 18px",
          },
          ".cm-foldGutter .cm-gutterElement": {
            // Tight around the arrow; the right side stays small because the
            // divider rule + the code's own left padding already sit beyond it.
            padding: "0 2px 0 4px",
          },
          // Fold markers: a muted down chevron when open, the accent side arrow
          // when collapsed, with a rounded hover shade behind the glyph.
          ".cm-foldMarker": {
            fontSize: "16px",
            lineHeight: "inherit",
            color: "var(--muted)",
            cursor: "pointer",
            borderRadius: "4px",
          },
          ".cm-foldMarker:hover": {
            color: "var(--fg)",
            background: "color-mix(in srgb, var(--fg) 12%, transparent)",
          },
          ".cm-foldMarker-closed": { color: "var(--accent)" },
          // The collapsed `…` placeholder: themed, no default light chip.
          ".cm-foldPlaceholder": {
            background: "none",
            border: "none",
            color: "var(--muted)",
            padding: "0 4px",
            cursor: "pointer",
          },
          ".cm-activeLineGutter": {
            backgroundColor: "color-mix(in srgb, var(--fg) 6%, transparent)",
            color: "var(--fg)",
          },
          // Text selection rides the calm --select blue (the T7.32 "selected"
          // token the render painter uses for the cursor<->render highlight),
          // not the warm --accent — so a selection reads as *selected* rather
          // than washing the notation in accent (T7.34e). `!important` is
          // required: CodeMirror's built-in `&light.cm-focused … ` selection
          // rule (#d7d4f0 — a pale near-white) is higher-specificity and always
          // matches (the editor is CM's "light" mode; the app themes via CSS
          // vars CM can't see), so a plain override never wins while focused.
          ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
            backgroundColor:
              "color-mix(in srgb, var(--select) 30%, transparent) !important",
          },
        }),
        syntaxHighlighting,
        diagnosticsExtension,
        // Core-driven autocomplete + inline operand hints (T7.24).
        completionExtension,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChange?.(update.state.doc.toString());
          }
          if (update.selectionSet || update.docChanged) {
            onCursor?.(update.state.selection.main.head);
          }
        }),
        // Focusing an editor makes its document active (active-follows-focus),
        // so the topbar name, Save, and Export track the focused file.
        EditorView.domEventHandlers({ focus: () => onFocus?.() }),
      ],
    });
  }

  onMount(() => {
    view = new EditorView({ state: buildState(doc), parent: container });
    view.focus();
  });

  // Push the latest classified tokens into the view as they arrive (and once the
  // view exists). Decorations remap through edits in between.
  $effect(() => {
    view?.dispatch({ effects: setTokens.of(tokens) });
  });

  // Likewise for diagnostics: underline squiggles + hover tooltips.
  $effect(() => {
    view?.dispatch({ effects: setDiagnostics.of(diagnostics) });
  });

  // And the completion vocabulary: the source reads it synchronously, so a fresh
  // compile's keyword/identifier set is in place before the next keystroke.
  $effect(() => {
    view?.dispatch({ effects: setCompletions.of(completions) });
  });

  // The autocomplete on/off setting (T7.24c): off silences the popup and inline
  // hints, leaving the rest of the editor untouched.
  $effect(() => {
    view?.dispatch({ effects: setCompletionEnabled.of(autocomplete) });
  });

  // Swap in a freshly-built state when a new load is requested (opening a file).
  // Keyed on the token so it fires once per load, not on every re-render. A new
  // state resets the undo history to the loaded document and clears decorations;
  // the following recompile re-pushes tokens/diagnostics for the new content.
  $effect(() => {
    if (!view || !loadRequest || loadRequest.token === lastLoadToken) return;
    lastLoadToken = loadRequest.token;
    view.setState(buildState(loadRequest.content));
    view.focus();
  });

  // Apply a format request (T7.25): replace the whole document in one
  // transaction. Unlike a load, this stays on the undo stack — Cmd/Ctrl-Z
  // reverts the format — and fires onChange so the doc store re-syncs. Keyed on
  // the token so it applies once. The cursor is clamped into the new text.
  $effect(() => {
    if (!view || !formatRequest || formatRequest.token === lastFormatToken)
      return;
    lastFormatToken = formatRequest.token;
    const { content } = formatRequest;
    if (view.state.doc.toString() === content) return;
    const head = Math.min(view.state.selection.main.head, content.length);
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: content },
      selection: EditorSelection.single(head),
    });
  });

  // Apply a selection requested from outside (a clicked render primitive),
  // clamped to the document, and bring it into view. Idempotent: skip the
  // dispatch when the cursor is already there, so the resulting cursor->render
  // highlight update doesn't bounce back into a re-dispatch loop.
  $effect(() => {
    if (!view || !selection) return;
    const len = view.state.doc.length;
    const from = Math.min(selection.from, len);
    const to = Math.min(selection.to, len);
    const cur = view.state.selection.main;
    if (cur.from === from && cur.to === to) return;
    view.dispatch({
      selection: EditorSelection.single(from, to),
      scrollIntoView: true,
    });
  });

  onDestroy(() => view?.destroy());
</script>

<!-- Zoom scales the code text: the editor inherits this font-size, so Cmd/Ctrl
     +/- on a focused editor grows/shrinks the code. 1em = base size. -->
<div class="editor" bind:this={container} style="font-size: {zoom}em"></div>

<style>
  .editor {
    height: 100%;
    overflow: auto;
    font-family: ui-monospace, monospace;
  }
  .editor :global(.cm-editor) {
    height: 100%;
  }
</style>
