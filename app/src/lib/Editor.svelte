<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { EditorState } from "@codemirror/state";
  import { EditorView, keymap } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";

  let {
    doc = "",
    onChange,
  }: { doc?: string; onChange?: (value: string) => void } = $props();

  let container: HTMLDivElement;
  let view: EditorView | undefined;

  onMount(() => {
    const state = EditorState.create({
      doc,
      extensions: [
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChange?.(update.state.doc.toString());
          }
        }),
      ],
    });
    view = new EditorView({ state, parent: container });
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
