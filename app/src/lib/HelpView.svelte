<script lang="ts">
  // The Help view (T7.29): a global-singleton, getting-started reference opened
  // as a tab from the bottom bar. Static how-to content — syntax basics, the
  // workspace, and keyboard shortcuts. Overlaps M8's T8.3 (richer docs); kept
  // self-contained so it themes with everything else.
  let { onActivate }: { onActivate?: () => void } = $props();

  // Keyboard shortcuts, grouped for the reference table. `mod` rows render the
  // platform-agnostic "Cmd/Ctrl" chord; a `note` tags platform-specific ones.
  const shortcuts: { keys: string[]; label: string; note?: string }[] = [
    { keys: ["mod", "O"], label: "Open a score or project" },
    { keys: ["mod", "⇧", "O"], label: "Open a folder", note: "desktop" },
    { keys: ["mod", "S"], label: "Save the active score" },
    { keys: ["mod", "B"], label: "Toggle the project dock" },
    { keys: ["mod", "W"], label: "Close the focused tab" },
    { keys: ["mod", "L"], label: "Select the current line" },
    { keys: ["mod", "Z"], label: "Undo" },
    { keys: ["mod", "+"], label: "Zoom the focused view in" },
    { keys: ["mod", "−"], label: "Zoom the focused view out" },
    { keys: ["mod", "0"], label: "Fit the focused view to width" },
    { keys: ["Tab"], label: "Accept the autocomplete suggestion" },
  ];
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="help" onpointerdown={() => onActivate?.()}>
  <article>
    <header class="intro">
      <h1>Welcome to cadtab</h1>
      <p>
        cadtab turns plain-text notation into engraved tablature. Type a score
        on the left and watch it render, live, on the right — no dragging notes
        onto a staff.
      </p>
    </header>

    <section>
      <h2>The workspace</h2>
      <p>
        Every pane is a <strong>tab</strong>. A score opens with an
        <strong>Editor</strong> and a <strong>Render</strong> side by side; the
        <strong>Preview</strong> tab shows the paginated print output.
      </p>
      <ul>
        <li>
          <strong>Groups.</strong> Drag the divider to resize. Use the tab-strip
          controls to <strong>split</strong> a group, <strong>maximize</strong>
          it, or open a new tab. Double-click a tab to maximize its group.
        </li>
        <li>
          <strong>Project dock.</strong> Toggle the left dock to browse the files
          in an open folder or project bundle.
        </li>
        <li>
          <strong>Bottom bar.</strong> The right-hand controls switch the theme,
          toggle <em>format-on-save</em> and <em>autocomplete</em>, and show the
          <em>problems</em> count — click it to list every warning and error and jump
          to its spot in the source.
        </li>
      </ul>
    </section>

    <section>
      <h2>Writing a score</h2>
      <p>
        A file opens with header directives, then a <code
          >score &#123; … &#125;</code
        >
        block holding the music:
      </p>
      <pre class="sample">{`title    "Cripple Creek"
composer "trad."
tempo    130

instrument banjo
tuning     openG
capo "5th string @ 2"

score {
  time 4/4
  default 1/8            // baseline duration for unmarked notes

  section "A"            // a rehearsal mark above the next bar
  3:0.t  2:0.i  1:0.m    // notes are auto-barred to fit the time
}`}</pre>

      <h3>Notes</h3>
      <p>
        A note is <code>string:fret</code> — string <code>1</code> is the
        highest-pitched. Add an optional right-hand finger (<code>.t</code>
        thumb, <code>.i</code> index, <code>.m</code> middle) and an optional duration:
      </p>
      <ul class="defs">
        <li>
          <code>3:0</code> — open 3rd string, at the running
          <code>default</code> duration
        </li>
        <li><code>1:7.m</code> — 1st string, 7th fret, middle finger</li>
        <li>
          <code>3:2_4</code> — a one-shot quarter note (<code>_N</code> overrides
          the default)
        </li>
        <li>
          <code>[1:0 5:0]_4</code> — a pinch/chord: one shared duration, notes
          in <code>[ ]</code>
        </li>
        <li><code>r_8</code> — an eighth rest</li>
        <li><code>3:2 ~ 3:2</code> — a tie, with <code>~</code></li>
      </ul>

      <h3>Techniques</h3>
      <p>
        Techniques are functions over the notes they mark —
        <code>hammer(3:0, 3:2)</code>, <code>pull(…)</code>,
        <code>slide(…)</code>,
        <code>bend(1:7)</code>, <code>choke(…)</code>, <code>ghost(…)</code>.
      </p>

      <h3>Structure</h3>
      <ul class="defs">
        <li><code>section "B"</code> — a rehearsal mark above the next bar</li>
        <li><code>chord "C"</code> — a chord symbol above the staff</li>
        <li>
          <code>repeat &#123; … &#125;</code> with
          <code>ending(1) &#123; … &#125;</code> — musical repeats and their endings
        </li>
        <li>
          <code>pickup &#123; … &#125;</code> — a partial pickup bar (anacrusis)
        </li>
        <li><code>loop 2 &#123; … &#125;</code> — unrolls its body twice</li>
        <li><code>measure &#123; … &#125;</code> — an explicit bar override</li>
      </ul>

      <h3>Reuse</h3>
      <p>
        Factor phrases into <code>def</code>s and pull shared libraries in with
        <code>import</code>. A file of <code>def</code>s with no
        <code>score</code> renders as a gallery of preview cards.
      </p>
    </section>

    <section>
      <h2>Keyboard shortcuts</h2>
      <table class="keys">
        <tbody>
          {#each shortcuts as s (s.label)}
            <tr>
              <td class="chord">
                {#each s.keys as k, i (i)}
                  {#if i > 0}<span class="plus">+</span>{/if}
                  <kbd>{k === "mod" ? "Cmd/Ctrl" : k}</kbd>
                {/each}
              </td>
              <td class="what">
                {s.label}
                {#if s.note}<span class="tag">{s.note}</span>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  </article>
</div>

<style>
  .help {
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow: auto;
    background: var(--bg);
    color: var(--fg);
    font-family: system-ui, sans-serif;
  }
  article {
    max-width: 46rem;
    margin: 0 auto;
    padding: 2rem 2.4rem 4rem;
    line-height: 1.55;
  }
  .intro {
    border-bottom: 1px solid var(--border);
    padding-bottom: 1.2rem;
    margin-bottom: 1.4rem;
  }
  h1 {
    margin: 0 0 0.4rem;
    font-size: 1.7rem;
  }
  .intro p {
    margin: 0;
    color: var(--muted);
    font-size: 1.02rem;
  }
  section {
    margin-top: 2rem;
  }
  h2 {
    margin: 0 0 0.7rem;
    font-size: 1.2rem;
  }
  h3 {
    margin: 1.3rem 0 0.4rem;
    font-size: 0.98rem;
  }
  p {
    margin: 0 0 0.7rem;
  }
  ul {
    margin: 0 0 0.7rem;
    padding-left: 1.2rem;
  }
  li {
    margin: 0.25rem 0;
  }
  ul.defs {
    list-style: none;
    padding-left: 0;
  }
  ul.defs li {
    padding: 0.15rem 0;
  }
  /* Inline code + the multi-line sample share the monospace + tinted surface. */
  code,
  .sample {
    font-family: ui-monospace, monospace;
    background: color-mix(in srgb, var(--fg) 7%, transparent);
    border-radius: 0.25rem;
  }
  code {
    padding: 0.05rem 0.3rem;
    font-size: 0.88em;
    color: var(--accent);
  }
  .sample {
    display: block;
    margin: 0 0 0.9rem;
    padding: 0.9rem 1.1rem;
    overflow-x: auto;
    font-size: 0.82rem;
    line-height: 1.5;
    color: var(--fg);
    border: 1px solid var(--border);
  }
  .keys {
    border-collapse: collapse;
    width: 100%;
  }
  .keys td {
    padding: 0.3rem 0.4rem;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  .keys .chord {
    white-space: nowrap;
    width: 1%;
  }
  .keys .what {
    color: var(--muted);
  }
  kbd {
    display: inline-block;
    font-family: ui-monospace, monospace;
    font-size: 0.78rem;
    padding: 0.1rem 0.4rem;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 0.3rem;
    background: color-mix(in srgb, var(--fg) 5%, var(--bg));
    color: var(--fg);
  }
  .plus {
    color: var(--muted);
    margin: 0 0.2rem;
    font-size: 0.75rem;
  }
  .tag {
    margin-left: 0.4rem;
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 0.3rem;
    padding: 0.02rem 0.3rem;
  }
</style>
