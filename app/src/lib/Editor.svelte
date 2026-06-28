<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { EditorState, EditorSelection } from "@codemirror/state";
  import {
    EditorView,
    keymap,
    drawSelection,
    dropCursor,
    highlightActiveLine,
  } from "@codemirror/view";
  import {
    defaultKeymap,
    history,
    historyKeymap,
    indentWithTab,
    selectLine,
  } from "@codemirror/commands";
  import { syntaxHighlighting, setTokens } from "./highlight";
  import {
    diagnostics as diagnosticsExtension,
    setDiagnostics,
  } from "./diagnostics";
  import type { Token, Diagnostic } from "./types";

  let {
    doc = "",
    onChange,
    onCursor,
    onFocus,
    tokens = [],
    diagnostics = [],
    selection = null,
    loadRequest = null,
  }: {
    doc?: string;
    onChange?: (value: string) => void;
    onCursor?: (pos: number) => void;
    onFocus?: () => void;
    tokens?: Token[];
    diagnostics?: Diagnostic[];
    selection?: { from: number; to: number } | null;
    loadRequest?: { content: string; token: number } | null;
  } = $props();

  let container: HTMLDivElement;
  let view = $state<EditorView | undefined>();
  // Highest load token applied, so a re-render doesn't re-replace the document.
  let lastLoadToken = -1;

  // Build a fresh editor state for `text`. Loading a document rebuilds the state
  // (rather than editing the current one) so the loaded file becomes the undo
  // baseline — there is no "undo back to the previous document".
  function buildState(text: string): EditorState {
    return EditorState.create({
      doc: text,
      extensions: [
        history(),
        // CM's own caret + selection layer (the native one only shows on focus
        // and can't be themed), a drop caret, and an active-line highlight.
        drawSelection(),
        dropCursor(),
        highlightActiveLine(),
        // Tab inserts indentation rather than moving focus out; Cmd/Ctrl-L
        // selects the whole line (Mod maps to Cmd on macOS, Ctrl elsewhere).
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          indentWithTab,
          { key: "Mod-l", run: selectLine },
        ]),
        // Bind the editor surface to the app's semantic theme tokens so it
        // re-themes (background, caret, selection) with everything else.
        EditorView.theme({
          "&": { backgroundColor: "var(--bg)", color: "var(--fg)" },
          ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--fg)" },
          ".cm-activeLine": {
            backgroundColor: "color-mix(in srgb, var(--fg) 6%, transparent)",
          },
          ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
            backgroundColor:
              "color-mix(in srgb, var(--accent) 25%, transparent)",
          },
        }),
        syntaxHighlighting,
        diagnosticsExtension,
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

<div class="editor" bind:this={container}></div>

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
