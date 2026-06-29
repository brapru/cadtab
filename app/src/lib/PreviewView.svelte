<script lang="ts">
  import { paginate } from "./core";
  import { renderPageToSvg } from "./svg";
  import { PDF_CONTENT_WIDTH } from "./sizing";
  import { debounce } from "./debounce";
  import type { Page } from "./types";

  // Print preview: the document's actual paginated print output — the same pages
  // the PDF export produces, rendered as light sheets. This paginates through the
  // core seam (a real second layout pass, not the screen render tree), so the
  // print top margin, header indent, and page breaks match the exported PDF.
  // Always light, regardless of the app theme.
  let {
    source = "",
    basePath = null,
    files = {},
    error = "",
    onActivate,
  }: {
    source?: string;
    basePath?: string | null;
    files?: Record<string, string>;
    error?: string;
    onActivate?: () => void;
  } = $props();

  type Ctx = { basePath: string | null; files: Record<string, string> };

  let pages = $state<Page[]>([]);
  // Latest-wins guard: a slow paginate must not clobber a newer one's pages.
  let seq = 0;

  async function repaginate(src: string, ctx: Ctx) {
    const mySeq = ++seq;
    try {
      const tree = await paginate(
        src,
        { size: "letter", contentWidth: PDF_CONTENT_WIDTH },
        ctx,
      );
      if (mySeq === seq) pages = tree.pages;
    } catch {
      if (mySeq === seq) pages = [];
    }
  }

  const debounced = debounce(
    (src: string, ctx: Ctx) => void repaginate(src, ctx),
    150,
  );

  $effect(() => {
    // Re-paginate (debounced) whenever the source or project context changes.
    debounced(source, { basePath, files });
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="preview" onpointerdown={() => onActivate?.()}>
  {#if pages.length > 0}
    <div class="pages">
      {#each pages as page, i (i)}
        <div class="sheet">
          <!-- Our own serializer output (text escaped in svg.ts), not user HTML. -->
          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
          {@html renderPageToSvg(page)}
        </div>
      {/each}
    </div>
  {:else if error}
    <p class="error">{error}</p>
  {/if}
</div>

<style>
  /* The sheets (.sheet) stay white — they are the printed output — but the
     surrounding backdrop tracks the theme so they aren't a harsh bright panel. */
  .preview {
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow: auto;
    padding: 1.5rem;
    background: color-mix(in srgb, var(--fg) 15%, var(--bg));
    display: flex;
    justify-content: center;
    align-items: flex-start;
  }
  /* Pages stack down the pane, each a discrete sheet. */
  .pages {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.5rem;
    width: 100%;
  }
  .sheet {
    background: #ffffff;
    box-shadow: 0 1px 6px rgba(0, 0, 0, 0.3);
    width: 100%;
    max-width: 720px;
  }
  /* Scale each page SVG to the sheet width while keeping its aspect ratio. */
  .sheet :global(svg) {
    display: block;
    width: 100%;
    height: auto;
  }
  .error {
    color: var(--fg);
    opacity: 0.7;
    padding: 1rem;
  }
</style>
