<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { EditorState } from "@codemirror/state";
  import {
    EditorView,
    keymap,
    drawSelection,
    dropCursor,
    highlightActiveLine,
  } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
  import { syntaxHighlighting, setTokens } from "./highlight";
  import {
    diagnostics as diagnosticsExtension,
    setDiagnostics,
  } from "./diagnostics";
  import type { Token, Diagnostic } from "./types";

  let {
    doc = "",
    onChange,
    tokens = [],
    diagnostics = [],
  }: {
    doc?: string;
    onChange?: (value: string) => void;
    tokens?: Token[];
    diagnostics?: Diagnostic[];
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
        keymap.of([...defaultKeymap, ...historyKeymap]),
        syntaxHighlighting,
        diagnosticsExtension,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChange?.(update.state.doc.toString());
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
