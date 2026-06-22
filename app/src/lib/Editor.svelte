<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { EditorState } from "@codemirror/state";
  import { EditorView, keymap } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
  import { syntaxHighlighting, setTokens } from "./highlight";
  import type { Token } from "./types";

  let {
    doc = "",
    onChange,
    tokens = [],
  }: {
    doc?: string;
    onChange?: (value: string) => void;
    tokens?: Token[];
  } = $props();

  let container: HTMLDivElement;
  let view = $state<EditorView | undefined>();

  onMount(() => {
    const state = EditorState.create({
      doc,
      extensions: [
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        syntaxHighlighting,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChange?.(update.state.doc.toString());
          }
        }),
      ],
    });
    view = new EditorView({ state, parent: container });
  });

  // Push the latest classified tokens into the view as they arrive (and once the
  // view exists). Decorations remap through edits in between.
  $effect(() => {
    view?.dispatch({ effects: setTokens.of(tokens) });
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
