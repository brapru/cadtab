<script lang="ts">
  import Editor from "./lib/Editor.svelte";
  import Tab from "./lib/Tab.svelte";
  import { compile } from "./lib/core";
  import { createLiveCompiler } from "./lib/live";
  import { debounce } from "./lib/debounce";
  import type { CompileResult } from "./lib/types";

  const initialDoc = "score {\n  3:0 2:0 1:0 5:0\n}\n";

  let result = $state<CompileResult | null>(null);
  let error = $state("");

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

  function recompile(source: string) {
    void live.run(source, { width: 800 });
  }

  const onChange = debounce((value: string) => recompile(value), 150);

  recompile(initialDoc);
</script>

<main>
  <h1>cadtab</h1>
  <div class="panes">
    <div class="editor-pane">
      <Editor
        doc={initialDoc}
        {onChange}
        tokens={result?.tokens ?? []}
        diagnostics={result?.diagnostics ?? []}
      />
    </div>
    <div class="render-pane">
      {#if result}
        <Tab tree={result.renderTree} />
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
