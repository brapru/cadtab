<script lang="ts">
  import Editor from "./lib/Editor.svelte";
  import Tab from "./lib/Tab.svelte";
  import { compile } from "./lib/core";
  import { createLiveCompiler } from "./lib/live";
  import { debounce } from "./lib/debounce";
  import { byteToCharIndex, charToByteIndex, spanToRange } from "./lib/spans";
  import { narrowestSpanAt } from "./lib/mapping";
  import type { CompileResult, Span } from "./lib/types";

  const initialDoc = "score {\n  3:0 2:0 1:0 5:0\n}\n";

  let result = $state<CompileResult | null>(null);
  let error = $state("");
  // The source the current result was compiled from, so cursor<->span conversions
  // line up with the spans in that render tree.
  let source = $state(initialDoc);
  let selection = $state<{ from: number; to: number } | null>(null);
  let activeSpan = $state<Span | null>(null);

  const live = createLiveCompiler(
    compile,
    (r) => {
      result = r;
      error = "";
    },
    () => {
      error = "core unavailable (no backend)";
    },
  );

  function recompile(src: string) {
    source = src;
    void live.run(src, { width: 800 });
  }

  const onChange = debounce((value: string) => recompile(value), 150);

  // Render -> source: a clicked primitive selects its source range in the editor.
  function handlePrimitiveClick(span: Span) {
    const range = spanToRange(byteToCharIndex(source), span);
    if (range) selection = range;
  }

  // Source -> render: the cursor lights up the primitive(s) sharing its range.
  function handleCursor(pos: number) {
    if (!result) return;
    const byte = charToByteIndex(source)[pos] ?? 0;
    activeSpan = narrowestSpanAt(result.renderTree, byte);
  }

  // Clicking empty render space (or Escape) drops the highlight, mirroring how
  // clicking off a note in the editor clears it. Primitive clicks stop
  // propagating, so this only fires for the background.
  function clearHighlight() {
    activeSpan = null;
  }

  recompile(initialDoc);
</script>

<main>
  <h1>cadtab</h1>
  <div class="panes">
    <div class="editor-pane">
      <Editor
        doc={initialDoc}
        {onChange}
        onCursor={handleCursor}
        {selection}
        tokens={result?.tokens ?? []}
        diagnostics={result?.diagnostics ?? []}
      />
    </div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="render-pane"
      onclick={clearHighlight}
      onkeydown={(e) => e.key === "Escape" && clearHighlight()}
    >
      {#if result}
        <Tab
          tree={result.renderTree}
          {activeSpan}
          onPrimitiveClick={handlePrimitiveClick}
        />
      {:else if error}
        <p class="error">{error}</p>
      {/if}
    </div>
  </div>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    margin: 0;
    font-family: system-ui, sans-serif;
  }
  h1 {
    margin: 0;
    padding: 0.5rem 1rem;
    font-size: 1.1rem;
  }
  .panes {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .editor-pane {
    flex: 1;
    border-right: 1px solid currentColor;
    min-width: 0;
  }
  .render-pane {
    flex: 1;
    padding: 1rem;
    overflow: auto;
  }
  .error {
    opacity: 0.7;
  }
</style>
