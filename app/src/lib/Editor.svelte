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
    tokens = [],
    diagnostics = [],
    selection = null,
  }: {
    doc?: string;
    onChange?: (value: string) => void;
    onCursor?: (pos: number) => void;
    tokens?: Token[];
    diagnostics?: Diagnostic[];
    selection?: { from: number; to: number } | null;
  } = $props();

  let container: HTMLDivElement;
  let view = $state<EditorView | undefined>();

  onMount(() => {
    const state = EditorState.create({
      doc,
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
      ],
    });
    view = new EditorView({ state, parent: container });
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

  // Apply a selection requested from outside (a clicked render primitive),
  // clamped to the document, and bring it into view.
  $effect(() => {
    if (!view || !selection) return;
    const len = view.state.doc.length;
    const from = Math.min(selection.from, len);
    const to = Math.min(selection.to, len);
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
