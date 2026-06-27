# cadtab — MVP Task Order

> The sequenced build plan for the MVP (Pillar A: DSL → tab renderer). Design decisions it
> implements live in [`DESIGN.md`](./DESIGN.md) (referenced as D#). Built **walking-skeleton
> first**: the thinnest end-to-end slice runs early, then each layer thickens behind a working
> contract.

## How to use this doc

- Work milestones **M0 → M8** in order. The spine **M0 → M1 → M2 → M3** is the critical path.
- Within a milestone, check off tasks top-to-bottom. The heavy "epic" tasks (lexer, parser,
  evaluator, auto-barring, beaming, painter, shell) are **pre-decomposed into lettered
  sub-tasks** (e.g. T1.4a–g); green-gate each sub-task. If any remaining task still feels too
  big mid-build, split it further the same way.
- **Do not start the next task/sub-task until the Definition of Done is green** (see below).
- Exact crate/library choices per area live in `DESIGN.md` §11c (the dependency stack, D40).

## Definition of Done (applies to EVERY task and sub-task)

A task is **done** only when all of the following pass — locally *and* in CI:

- **Rust:** `cargo fmt --check` · `cargo clippy --all-targets -- -D warnings` · `cargo test`
- **TS/Svelte:** `prettier --check` · `eslint` · `svelte-check` · `vitest run`
- **The task introduced tests** covering its new behavior (unit / snapshot / component as fits).
- **Build succeeds** for every target the task touches (core, wasm, tauri, web).

> 🔒 **Green-gate:** no new task begins while any of the above is red. One aggregate command —
> `just check` (or `npm run check`) — runs the whole gate; CI runs the same command.

## Testing strategy

- **`cadtab-core` is pure and UI-free** → the entire `source → render tree` pipeline is
  unit/snapshot testable headlessly. This is the bulk of the test value; lean on it.
- **Golden/snapshot tests** (`insta`) for lexer, parser (incl. error-recovery corpora),
  evaluator (`source → model`), and layout (`model → render tree`).
- **serde round-trip tests** for every type crossing the Rust↔TS boundary (render tree,
  diagnostics, tokens) — guards the contract.
- **Property tests** (`proptest`) for the fiddly pure bits: rational `Duration` math, span
  arithmetic, auto-barring beat accumulation.
- **Frontend component tests** (`vitest` + `@testing-library/svelte`) for the SVG painter,
  highlighting, diagnostics, and bidirectional mapping.
- **§6's example program** (Syntax Showcase) is the canonical end-to-end fixture, reused across
  milestones as it becomes parseable → compilable → renderable.

---

## M0 — Foundations & Walking Skeleton

**Goal:** typing `3:0` in the real app shows one fret number on one string line, live, on
**both** desktop and web — and the full quality gate + CI is green. Validates every integration
boundary (core ↔ tauri ↔ wasm ↔ svelte ↔ svg) on day one.

- [x] **T0.1 — Workspace scaffold.** Cargo workspace + Svelte/Vite app + Tauri 2.
  - [x] T0.1a — Workspace root + `cadtab-core` lib crate with a passing trivial test.
  - [x] T0.1b — `src-tauri` crate via Tauri 2 init; blank window runs (`cargo tauri dev`).
  - [x] T0.1c — `app/` Svelte + Vite + TS scaffold wired as the Tauri frontend.
  - [x] T0.1d — `cadtab-wasm` crate skeleton (wasm-bindgen) that builds.
  - *Tests:* trivial `cadtab-core` unit test proving `cargo test` runs.
- [x] **T0.2 — Quality gates (before real code).** Stand up the full DoD gate.
  - [x] T0.2a — Rust: `rustfmt.toml`; clippy `-D warnings`; `insta` + `proptest` deps; `cargo test` wired.
  - [x] T0.2b — TS: prettier, eslint, `svelte-check`, vitest configs.
  - [x] T0.2c — Aggregate `just check` / `npm run check` runs the whole gate; `CONTRIBUTING.md` documents it.
- [x] **T0.3 — CI/CD.** GitHub Actions: run `check` (Rust fmt/clippy/test + TS lint/check/test +
      builds) on push/PR, with caching. **Must be green before any feature task.**
- [x] **T0.4 — Core API contract (stub).** Define `compile(source, LayoutConfig) ->
      CompileResult { render_tree, diagnostics, tokens }` in `cadtab-core`, returning a hardcoded
      trivial render tree (one string line + one fret-number `Text`).
  - *Tests:* stub returns expected shape; **serde round-trip** for render-tree/diagnostic/token types.
- [x] **T0.5 — Tauri command + TS `core` adapter.** Expose `compile` as a Tauri command; thin
      `core.compile()` TS adapter (D27) called from Svelte.
- [x] **T0.6 — Minimal SVG painter.** Render the stub tree (Line + Text) to SVG with `viewBox`
      scaling (D22). → **walking skeleton visible on desktop.**
  - *Tests:* painter component test — known tree → expected SVG nodes.
- [x] **T0.7 — CodeMirror + live loop.** Embed CodeMirror 6; debounced (~150ms) `core.compile`
      on edit; render the result; latest-wins/drop-stale (D27, D31).
  - *Tests:* adapter stale-drop unit test.
- [x] **T0.8 — WASM backend parity.** Lock the dual backend on day one (D4).
  - [x] T0.8a — `cadtab-wasm` exposes `compile` via wasm-bindgen; serde round-trip of `CompileResult`.
  - [x] T0.8b — TS `core` adapter dispatches Tauri-vs-WASM behind one interface (env detection).
  - [x] T0.8c — Web (vite) build renders the same stub skeleton; add wasm + web builds to CI.

**DoD M0:** live stub render on desktop **and** web; CI green across fmt/lint/test/build for all
targets.

---

## M1 — Language front-end (lex → parse → AST)

**Goal:** the full grammar parses into a span-bearing AST, *resiliently*, with diagnostics.
Entirely headless and test-driven (D18, D19, D20).

- [x] **T1.0 — Start `docs/GRAMMAR.md` (living, incremental).** Pin the confident core first
      (the §6 subset: notes, durations, chords, blocks); mark uncertain bits **provisional** and
      grow it construct-by-construct *alongside* the lexer/parser — not all up front. Captures
      EBNF + a precedence table (`_dur` suffix, `.mark`, `.index`, `~`, `...`, calls) and settles
      **tuplet syntax** (the D11 TBD) when that construct is actually built. A living test-oracle,
      not a one-way door — snapshot tests keep iteration cheap. *(Co-evolves with T1.2/T1.4.)*
- [x] **T1.1 — Source & diagnostics infra.** `Span` (byte offsets) + source map; `Diagnostic
      { severity, span, message, help }` (D31). Spans are mandatory on all nodes (D20).
  - *Tests:* span arithmetic property tests.
- [x] **T1.2 — Hand-rolled lexer.** Emits classified tokens (for highlighting, D27) + spans.
  - [x] T1.2a — `Token`/`TokenKind` enum with highlight classification + span.
  - [x] T1.2b — Scanner skeleton: cursor, whitespace, `//` + `/* */` comments, span emission.
  - [x] T1.2c — Literals (ints, strings), identifiers + keyword recognition.
  - [x] T1.2d — Music tokens: `:` separator, `_dur` suffix (incl. tuplet marker per T1.0), marks `.t/.i/.m/.d/.u`, `~`.
  - [x] T1.2e — Delimiters/operators: `[] {} ()`, `...`, index `.` (`repeat`/`ending`/`loop` are keywords, T1.2c).
  - [x] T1.2f — Error tokens + lexer diagnostics.
  - *Tests:* snapshot lex of §6 + edge/error cases per sub-task.
- [x] **T1.3 — AST types.** All node kinds, every node span-bearing.
- [x] **T1.4 — Recursive-descent parser (+ Pratt).** Resilient → partial AST + multiple diagnostics (D19).
  - [x] T1.4a — Skeleton: token cursor, lookahead, span tracking, diagnostic sink, **recovery infra** (sync points, error nodes).
  - [x] T1.4b — Top-level declarations: `title`/`composer`/`tempo`, `instrument`, `tuning`, `capo`, `import`.
  - [x] T1.4c — `score` / `measure` / `pickup` / `repeat` (musical) blocks + nested `ending(n){}` voltas.
  - [x] T1.4d — Events: note literal (`string:fret` + mark + `_dur`), chord `[…]`, rest, tie `~`.
  - [x] T1.4e — Expressions (Pratt): idents, calls, indexing `.N`/`len`, spread `...`, precedence per GRAMMAR.md.
  - [x] T1.4f — `def` / `let` / `loop N` (unroll).
  - [x] T1.4g — Error-recovery corpus + multi-diagnostic tests.
  - *Tests:* golden ASTs for a valid-program corpus (incl. §6); recovery corpus (T1.4g).

**DoD M1:** §6 + corpus parse to expected ASTs; recovery corpus yields expected diagnostics.

---

## M2 — Semantic core (resolve → typecheck → eval → model)

**Goal:** `source → musical model` for the whole language (D5–D17, D32–D39).

- [x] **T2.1 — Instruments, tunings, pitch.** Builtin banjo (Open G `gDGBD`) + guitar (`EADGBE`);
      `tuning` override (D35); pitch derivation `open_pitch[string] + fret`; 1-based→Vec mapping
      (D37); bounds validation (string in range, fret ≥ 0) → diagnostics.
  - *Tests:* pitch-derivation table; invalid-position diagnostics.
- [x] **T2.2 — Name resolution.** Lexical scopes; `def`/`let`; `import` (desktop fs + embedded
      stdlib, D38); shadowing; unresolved-name diagnostics.
- [x] **T2.3 — Minimal static type checker (D15).** Value kinds Int/Duration/Position/Note/Phrase;
      arity + kind checks; spread/index typing; diagnostics with `help`.
  - *Tests:* type-error corpus → expected diagnostics.
- [x] **T2.4 — Evaluator.** AST → musical-model values.
  - [x] T2.4a — Value types (Int/Duration/Position/Note/Phrase) + evaluation environment/scopes.
  - [x] T2.4b — Event eval: notes, chords (shared duration, D39), rests; sticky-duration threading (D11).
  - [x] T2.4c — `def` definition + call expansion → `Phrase` splicing (D14).
  - [x] T2.4d — `loop N` unroll expansion.
  - [x] T2.4e — Phrase indexing `.N` + `len`; spread `...` (D17).
  - [x] T2.4f — Technique fns → `Technique` annotations w/ target-note rules (D8); `~` → `tie` flag (D36).
  - *Tests:* `source → model` snapshots across the feature matrix.
- [x] **T2.5 — Auto-barring + pickup + repeats.**
  - [x] T2.5a — Beat accumulator over the event stream (rational time, per `time`).
  - [x] T2.5b — Bar splitting into `Measure`s + barline insertion (D12).
  - [x] T2.5c — Explicit `measure {}` override interplay.
  - [x] T2.5d — `pickup {}`: excluded from fill check, offset flag (D33).
  - [x] T2.5e — `repeat {}` → `repeat_start/end`; `ending(n){}` → volta routing + `ending` attrs (D32); meter changes.
  - [x] T2.5f — Over/under-full diagnostics with `help`.
  - *Tests:* barring corpus — pickups, meter changes, over/under-full errors.
- [x] **T2.6 — Metadata (D34).** `title`/`composer`/`tempo` → `ScoreMeta`.
- [x] **T2.7 — Stdlib licks.** Forward/backward/alt-thumb/Foggy Mountain embedded via
      `include_str!` (D16, D29); available by default.
  - *Tests:* each stdlib lick expands correctly.

**DoD M2:** §6 compiles to a golden `Score` model; feature-matrix + error corpora green.

---

## M3 — Layout engine (model → render tree)

**Goal:** a fully positioned, width-responsive render tree (D22–D25). Still headless.

- [x] **T3.1 — Render-tree types (final).** `System → MeasureBox → Primitive`, logical coords,
      serde, spans (D22). Supersede the M0 stub types.
  - *Tests:* serde round-trip.
- [x] **T3.2 — Vertical layout.** String lines; header (title/tempo/tuning/capo); fret-number
      placement with line-break-behind-number; string→line mapping (D37).
  - *Tests:* render-tree snapshot for a simple measure.
- [x] **T3.3 — Horizontal layout.** Time-proportional spacing within measures (D24); barlines;
      repeat barlines + ending (volta) brackets (D32); pickup offset (D33).
- [x] **T3.4 — Line-breaking.** Greedy wrap of measures into systems given `LayoutConfig.width`
      (D23, D24).
  - *Tests:* same model at two widths → different system counts.
- [ ] **T3.5 — Stems + beams (the fiddly one, D25).** Sub-tasked aggressively; heavy test coverage.
  - [x] T3.5a — Beat grouping: partition a measure's notes into beam groups by beat.
  - [x] T3.5b — Stem geometry (direction/length, below the numbers per tab convention).
  - [x] T3.5c — Primary beams (slope, thickness) across a group.
  - [x] T3.5d — Flags for unbeamed/solo notes.
  - [x] T3.5e — Dotted notes + tuplet bracketing. *(Dots done; tuplet brackets deferred until
        tuplet syntax is pinned — D11 TBD, currently unrepresentable in model/parser.)*
  - [x] T3.5f — Rests within/between beam groups.
  - *Tests:* render-tree snapshots across a rhythm matrix, per sub-task.
- [x] **T3.6 — Marks & paths.** T/I/M, strum arrows, technique marks (h/p/sl), ties, bends, choke
      as `Text`/`Path` primitives — each span-tagged (D20). *(Ghost technique drawn as no mark
      yet; cross-measure ties deferred.)*

**DoD M3:** §6 → render-tree golden snapshot; width-reflow + rhythm cases green.

---

## M4 — Real frontend & the live loop

**Goal:** swap the stub for the real core and deliver the slick live editor (D20, D26, D27, D31).

- [x] **T4.1 — Wire real `compile`** over IPC + WASM; latest-wins debounced async; error paths.
  - *Tests:* adapter stale-drop / error-handling unit tests.
- [x] **T4.2 — Full SVG painter.** All primitive kinds; `viewBox` zoom; theming.
  - [x] T4.2a — Line primitives (string lines, barlines, stems, beams).
  - [x] T4.2b — Text primitives (fret numbers, T/I/M, strum, labels) + role-based styling.
  - [x] T4.2c — Path primitives (ties, slides, bends, choke arcs).
  - [x] T4.2d — `viewBox` zoom + theming tokens. *(Painter capability + tokens; the
        zoom-control UI and light/dark toggle are wired later in T4.6c/T4.6d.)*
  - *Tests:* component test per primitive kind.
- [x] **T4.3 — Syntax highlighting** from Rust tokens → CodeMirror decorations (D27).
- [x] **T4.4 — Diagnostics UI.** Squiggles + hover tooltips; **best-effort partial render** on
      error (D31).
  - *Tests:* diagnostics → squiggles; render still shows valid parts.
- [x] **T4.5 — Bidirectional mapping (D20).** Click primitive → editor selection; cursor move →
      highlight primitives.
  - *Tests:* span↔primitive lookup unit tests.
- [x] **T4.6 — Shell & polish.**
  - [x] T4.6a — Split-pane (editor | render) with drag resize.
  - [x] T4.6b — Responsive reflow on resize (re-layout via width, debounced, D23).
  - [x] T4.6c — Zoom controls + fit-to-width.
  - [x] T4.6d — Theme (light/dark) + visual polish pass.
  - *Note:* editor cursor/selection basics (`drawSelection`, `dropCursor`,
    active-line, autofocus) were pulled forward during T4.3.
- [ ] **T4.7 — Post-polish fixes & refinements (from first real-use review).** Exercising the
      app end-to-end after T4.6 surfaced a batch of render, editor, and shell issues. Tracked
      here so the spine does not advance past M4 with known regressions. Green-gate each.
  - [x] T4.7a — **Surface semantic diagnostics.** `compile()` skipped name resolution *and* the
        type checker, so unknown-name/type errors never reached the editor (a bare `gibberish`
        showed nothing). Wired both passes in (parse → resolve → typecheck → eval); stdlib lick
        names seeded as ambient so they still resolve.
  - [x] T4.7b — **Desktop squiggles render.** WKWebView (Tauri's macOS webview) ignores the
        `text-decoration: wavy` shorthand; added `-webkit-text-decoration`. Squiggles were
        web-only before. *(Same WKWebView CSS exposure applies to T4.7i.)*
  - [x] T4.7c — **Connected stems.** Stems now hang from a slight gap below each event's lowest
        fret number to the beam line (was a fixed band): reaches upper-string notes, no longer
        overlaps the 5th-string number.
  - [x] T4.7d — **Beam thickness & flush join.** Thinner beams (0.18), butt line caps (no
        rounded overshoot past the outer stems), beam top edge flush with the stem ends.
  - [x] T4.7e — **Durations: `default` baseline + one-shot `_N` (revised D11).** Dropped the
        Lilypond sticky-on-override model that competed with `default`; `_N` no longer threads
        forward. DESIGN/GRAMMAR + showcase updated.
  - [x] T4.7f — **Strip per-line tuning.** Removed the per-line `StringLabel` prims from
        `build_system`; tuning now shows once in the header only.
  - [x] T4.7g — **Header layout (inline details row).** Collapsed the stacked detail rows into a
        single `♩=tempo · instrument · tuning · capo` row. *(Superseded by T4.7u's lead-sheet
        redesign.)*
  - [x] T4.7j — **Tab key indents.** Added `indentWithTab` to the editor keymap so Tab inserts
        indentation instead of moving focus out.
  - [x] T4.7k — **Keyboard zoom.** Cmd/Ctrl +/- zoom and Cmd/Ctrl 0 fits, wired to the existing
        zoom controls with `preventDefault` to override native page zoom.
  - [x] T4.7l — **Launch desktop maximized.** `maximized: true` + a 1200×800 restore size in
        `tauri.conf.json`.
  - [x] T4.7o — **Secondary beams for 16ths/32nds.** A beamed group now draws a primary beam
        plus a beam per higher level over each maximal run of `flag_count ≥ level` (stacked
        above the primary, since stems point down), with partial-beam stubs for isolated values.
        *(Known edge: a 32nd on a length-clamped 5th-string stem may not reach its 3rd beam —
        rare; revisit if 32nds get real use.)*
  - [x] T4.7p — **Dense-rhythm crowding.** Event spacing is time-proportional but now floored at
        `MIN_EVENT_GAP` (0.9, just under an eighth's 1.0), so 16th/32nd runs no longer pack their
        fret numbers together while eighths-and-longer keep proportional spacing.
  - [x] T4.7u — **Header redesign (lead-sheet style) + whole-sheet serif.** Supersedes the
        T4.7g inline details row with a traditional banjo lead-sheet header (ref:
        `docs/example-header.png`): centered title + bold composer, a left-aligned tuning block
        (tuning **name** over a circled-number grid `①=D ③=G ⑤=g / ②=B ④=D`), a tempo line with
        the ♩ glyph, and a capo line; all rendered sheet text set in serif. Built in two steps:
        (1) **tuning-name plumbing** — carry a tuning display name through `Instrument`/eval
        (`with_tuning("doubleC")` → "Double C", builtin defaults → "Open G"/"Standard"); (2)
        **header + serif** — rework `build_header` (`layout.rs`), new `TextRole`s
        (TuningName/TuningString/Tempo/Capo, drop Details), left-anchored header roles +
        `font-family: serif` on `.tab text` (`Tab.svelte`). Decisions made with the user:
        plumb the tuning name now; serif across the whole sheet; instrument name stays lowercase.
  - [x] T4.7v — **Cmd/Ctrl-L selects the line.** Added `selectLine` (`Mod-l`) to the editor
        keymap (Cmd on macOS, Ctrl elsewhere).
  - [x] T4.7w — **Close the staff on the left.** Each system's left edge now draws a barline
        (pickups stay open), so wrapped lines read as finished measures.
  - [x] T4.7x — **Time signature at the start.** Stacked numerator/denominator drawn at the first
        measure and at every meter change; digit gap is fixed (string-count-independent) and a
        full leading pad clears the first note.
  - [x] T4.7y — **Feature-rich `just dev` default.** Replaced the bare starter doc with a
        banjo/openG score (title/composer/tempo/capo, time signature, beamed bars) so the app
        opens showing the current feature set.
  - *Parked:* the showcase still emits 3 under-full-bar warnings on inherently-partial demo
    blocks (two voltas + the explicit `measure {}` fragment). Whether voltas / explicit measures
    should trigger under-full diagnostics at all is a diagnostics-quality question → revisit in
    T8.1 (and showcase metric cleanup in T8.3).
  - **Deferred render/UI items → M7.** The unfinished T4.7 render-quality and UI-polish items —
    **s** (intra-measure spacing), **t** (justify systems + pin page width), **h** (highlight
    palette), **i** (tooltip readability), **m** (diagnostics panel), **n** (accent pass),
    **r** (highlight treatment), **q** (structural bidirectional mapping) — were promoted to
    **M7 — Workspace shell & UI polish**, scheduled after M5 and the notation features (M6) per the
    re-sequencing decision. Their original IDs are retained there for continuity.

**DoD M4:** the live editor works end to end on desktop + web; component/integration tests green.
*(The deferred T4.7 render/UI items are tracked under M7, not gating M4.)*

---

## M5 — Persistence & export

**Goal:** open/save and share (D29, D30, D38).

- [x] **T5.1 — Open/Save `.ctab` + project bundle.** Desktop fs dialogs (single `.ctab`; folder
      projects). Web: File System Access API / download-upload for a single `.ctab` **and** a
      **project bundle** — one file serializing `{ entry, files }` (JSON map for MVP) so a complete
      multi-file project opens in the browser (D38).
  - *Tests:* save→open round-trip for both a single file and a bundle.
- [x] **T5.2 — `import` resolution via a file-provider.** Resolve `import` in core through a
      file-provider abstraction (path → contents), not fs-coupled: desktop = real fs (multi-file);
      web = in-memory map from the loaded bundle; embedded stdlib available on both (D38). This
      abstraction is what makes web multi-file possible and keeps the M7 project dock/tabs
      cross-platform.
  - *Tests:* headless resolution against an in-memory provider — stdlib, bundle, and
    missing/unresolved-file cases.
- [x] **T5.3 — Export SVG + PNG (D30).** Render tree → SVG string → PNG raster. *(PDF is also an
      MVP export — tracked as T7.9, post-M6; see D30.)*
  - *Tests:* export emits valid SVG; PNG non-empty.
- [x] **T5.4 — New-from-template.** Toolbar "New…" dropdown with banjo/guitar/blank starter
      scaffolds (compile-checked). *(Recent files deferred to M7's project dock, where it's
      cross-platform; web has no persistent paths.)*

**DoD M5:** ✅ round-trip persistence (single + bundle) + SVG/PNG export; green. *(Verify the file
dialogs / PNG raster in `just dev` / `just web` — they can't run headless.)*

---

## M6 — Notation features

**Goal:** richer notation above the staff — section labels, chord symbols, bar numbers — plus
user-defined tunings. Each spans the language (parser/eval) and the layout engine and emits new
above-staff render primitives. *(Newly identified mid-M4; sequenced after persistence per the
re-ordering decision so M5 ships first and export later covers these.)*

- [ ] **T6.1 — Section labels (rehearsal marks).** Mark the start of a section with a label drawn
      above the staff — e.g. the A part, B part, Chorus. Language: a marker that attaches a label
      to a measure boundary. Layout: text above the staff at that measure, span-tagged for
      bidirectional mapping. (Banjo tunes are commonly split into A/B parts.)
- [ ] **T6.2 — Chord symbols over bars.** Place a chord name (G, C, D7…) at the start or a beat
      within a bar so the progression sits above the tab. Language: a chord-annotation construct
      positioned at a beat. Layout: text above the staff aligned to that beat; span-tagged.
- [ ] **T6.3 — Bar numbering.** Number measures above the staff. Default: number only the first
      bar of each system line. Options: number every bar; turn all numbering off. Language: a
      directive to set the mode (e.g. `barnumbers lines|all|off`). Layout: a small number above
      the chosen measures.
- [ ] **T6.4 — Custom (user-defined) tunings.** Beyond the builtin named tunings, let a user
      define their own per-string tuning and have it drive pitch derivation and the header tuning
      grid. Extends T2.1/T2.7 tuning resolution; needs a display name (or "Custom") for the
      header. Language: a `tuning` form taking an explicit per-string spec.

**DoD M6:** section labels, chord symbols, bar numbering, and a custom tuning all parse, render
above/within the staff, and round-trip; golden snapshots + error corpora green.

---

## M7 — Workspace shell & UI polish

**Goal:** the post-M5 UI work — a Zed-inspired workspace shell (view registry + editor groups:
project dock, multi-file tabs, a slick bottom bar, render + print preview) **plus** the deferred
T4.7 render-quality and cohesion work — batched once persistence/export (M5) and the notation
features (M6) are in. The shell rests on the **D41** abstraction (see `DESIGN.md` §11d): a registry
of views (global-singleton vs document-bound) placed into editor groups (panes of stacked tabs)
that split, resize, and maximize — **no free-floating docking**. Build the foundation (T7.1) first;
the dock/tabs/render/preview are views on top; then the cohesion pass styles the result. Shell
chrome is universal (desktop + web), and multi-file projects work on every target (D38: live fs on
desktop, project bundle on web) — the only nuance is how the dock's tree is sourced (live folder on
desktop / Chromium-web; uploaded/exported bundle on Firefox). Migrated T4.7 items keep their IDs.

**Workspace shell (D41 — view registry + editor groups):**

- [ ] **T7.1 — Shell foundation: view registry + editor-groups layout.** The abstraction the rest
      of the shell stands on (D41): a registry of *views* (`id`, title, icon, mount/unmount,
      serializable state), classed as **global singletons** (dock, bottom bar) or **document-bound**
      (editor, render, preview, looper). Layout = **editor groups** — panes holding stacked tabs —
      with split, move-tab-between-groups, resize, and **maximize ("zoom") a group**. Generalizes
      today's editor|render split (its N=2, one-tab-each case). No free-floating docking (deferred).
- [ ] **T7.2 — Left project dock + Cmd/Ctrl-B.** *(global-singleton view)* Collapsible left dock
      showing project structure, toggled by Cmd/Ctrl-B and a bottom-bar button. The file tree comes
      from the project/import model (M5, D38) — a live folder on desktop / Chromium-web, or the
      loaded project bundle on Firefox.
- [ ] **T7.3 — Bottom status bar (slick, minimal, non-invasive).** *(global-singleton view)* A
      small bottom bar hosting the dock toggle and the diagnostics button (T4.7m); sets the
      bottom-control styling — small, unobtrusive, out of the way. Pairs with T4.7m and T4.7n.
- [ ] **T7.4 — Editor views + multi-file tabs.** *(document-bound)* Each open `.ctab` is an editor
      view; `import`ed files open as tabs across the groups. Depends on M5 import / multi-file;
      tab/group mechanics come from T7.1.
- [ ] **T7.5 — Render as a document-bound view.** Make the render a document-bound view placeable
      in any group, so "file + its render" sits side by side and file A's / file B's renders can
      coexist. Resize/reposition come from the group layout (T7.1) — no bespoke docking.
- [ ] **T7.6 — Print-preview view.** *(document-bound)* The final printed (light) output regardless
      of editor theme. *Recommendation:* implement as a mode reusing the export styling (T5.3),
      **not** a separate pipeline, so it isn't duplicative of the live render.
- [ ] **T7.9 — PDF export (paginated, MVP — D30).** The MVP's third export format and the
      distribution standard for tab. Unlike SVG/PNG (one continuous canvas, shipped in T5.3), PDF
      needs **pagination**: fixed Letter/A4 pages, systems packed per page, margins, and a per-page
      sheet header. This is layout work, not a serializer — it builds on the pinned-page width
      (T4.7t) and reuses the print styling (T5.3 / preview T7.6). Sequenced here (post-M6) so it
      paginates the *final* above-staff layout once. Save via the existing io seam (binary write on
      desktop, download on web).
  - *Tests:* page-break placement (systems-per-page) golden cases; multi-page doc emits N pages;
    one-page doc emits one; valid PDF bytes (header + page count).

**Editor & theme:**

- [ ] **T7.7 — Editor line numbers + gutter divider.** CodeMirror `lineNumbers()` gutter with a
      divider rule between the gutter and the code text.
- [ ] **T7.8 — Dark theme by default.** Default the app to the dark theme (keep the light / system
      toggle).

**Layout (migrated from T4.7):**

- [ ] **T4.7t — Justify systems + pin page width.** A line holding only one (or a few) measures
      renders at its natural width, leaving the system short. Stretch measures/events to fill the
      system width (justified systems). *Includes* **pinning the page width to the layout target**
      so the header (centred on the page) and the zoom stop reflowing as measures are added — root
      cause is `width = overall_width(...)` in `layout()` (content-derived), fix is
      `overall_width(...).max(config.width)`, then justify within that fixed page. Relates to
      T3.3/T3.4 and T4.7s.
- [ ] **T4.7s — Even out intra-measure spacing.** A bar's last note gets trailing space equal to
      its full duration, reading as more room on its right than the small leading pad. Spacing
      pass: revisit trailing-space vs leading-pad symmetry / even distribution. Pairs with T4.7t.

**Diagnostics (migrated from T4.7):**

- [ ] **T4.7i — Diagnostic tooltip readability.** Currently white-on-white until selected; give it
      themed background/foreground/border keyed to the semantic tokens (WKWebView caveat per
      T4.7b).
- [ ] **T4.7m — Diagnostics panel + bottom button.** A warning/error count button (lives in the
      T7.3 bottom bar) that opens an exhaustive list; clicking an entry jumps the editor selection
      to its span. *(This is the "error diagnostic button down below" from the notes.)*

**Cohesion (migrated from T4.7):**

- [ ] **T4.7h — Syntax-highlighting palette.** Muted two-tone palette (desaturated blue structure,
      warm tan numbers, muted green strings, gray italic comments); replace the dramatic
      full-width active-line bar with a faint left-edge tick so the cursor stays readable.
- [ ] **T4.7r — Bidirectional highlight treatment.** *(open decision)* The active cursor↔primitive
      highlight reuses the orange `--accent` and reads wrong. Pick a calmer treatment (e.g.
      desaturated fill vs underline vs halo, and which token); touches the theme accent token +
      `.active` styles in `Tab.svelte`. T4.7q reuses whatever is chosen.
- [ ] **T4.7n — Accent/detail cohesion pass.** A coherent accent/detail pass across topbar,
      toolbar, gutter, panels, **the new dock / tabs / bottom bar**, and M5's open/save/export
      buttons so the whole UI reads as one design. Umbrella for h, i, r and the shell chrome.
- [ ] **T4.7q — Structural bidirectional mapping.** Cursor↔render mapping (T4.5) only covers
      span-bearing text/notes; thread spans onto repeat barlines, ending (volta) brackets, and
      `measure {}` boxes, and extend the highlight (reusing T4.7r's treatment) to line/box prims so
      clicking or cursoring a repeat / ending / measure lights it up. *(Extends T4.5.)*

**DoD M7:** the Zed-style shell (dock, tabs, bottom bar, dockable render, preview) works on desktop
+ web; justified systems with a fixed page; **paginated PDF export (T7.9)**; readable diagnostics +
panel; dark-by-default cohesive themed UI; structural elements participate in bidirectional mapping.
Green.

---

## M8 — Hardening & MVP polish (ship)

**Goal:** a polished, packaged MVP on desktop + web.

- [ ] **T8.1 — Diagnostic quality pass.** Player-facing wording + `help` text for common errors.
- [ ] **T8.2 — Performance.** Debounce tuning; profile large docs; fix hotspots (revisit D21 only
      if needed).
- [ ] **T8.3 — Content.** Sample songs, broaden the stdlib, a short tutorial/getting-started doc.
- [ ] **T8.4 — Packaging.** Tauri desktop bundles (mac/win/linux) + deployed web build; release CI.
- [ ] **T8.5 — E2E smoke test** (Playwright/WebDriver) of the core flow.

**DoD / MVP ship:** author `.ctab` banjo tab live (highlighting, diagnostics, rhythm via
stems+beams, licks, repeats, pickups, metadata), bidirectional mapping, open/save, export
SVG+PNG+PDF (paginated, T7.9) — on desktop and web. CI green; packaged.

---

## Critical path & parallelism

- **Spine (sequential):** M0 → M1 → M2 → M3. Each strictly needs the prior.
- **M4** needs M3 (render tree) + M0 (shell). **M5 → M6 → M7 → M8** follow M4 in that order:
  persistence/export first, then the notation features, then the batched layout & UI polish, then
  ship. *(Re-sequenced mid-M4: the unfinished T4.7 render/UI work was deferred to M7 so a saveable
  product and the new notation features land first.)*
- **Parallelizable within milestones:** lexer (T1.2) vs AST types (T1.3); instrument table (T2.1)
  vs stdlib (T2.7); painter primitive kinds (T4.2) across types; the M6 notation features are
  largely independent of one another (custom tunings is the smallest; section labels / chords /
  bar numbers share the above-staff text machinery).

## Risk register

| Risk | Where | Mitigation |
|------|-------|------------|
| Beam/stem geometry is fiddly | T3.5 | Sub-task hard; rhythm snapshot matrix; isolate as pure fn |
| Resilient parser recovery quality | T1.4 | Error-recovery corpus from day one; AST boundary keeps parser swappable (D18) |
| Bidirectional mapping correctness | T4.5 | Spans threaded from M1 (D20); dedicated lookup tests |
| WASM/Tauri parity drift | T0.8 | Both backends in CI from M0; one shared `core.compile` contract |
| Layout reflow performance | T3.4/T8.2 | Profile; debounce; full-recompile is fine until proven otherwise (D21) |
