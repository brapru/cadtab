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
- [x] **T3.5 — Stems + beams (the fiddly one, D25).** Sub-tasked aggressively; heavy test coverage.
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
- [x] **T4.7 — Post-polish fixes & refinements (from first real-use review).** Exercising the
      app end-to-end after T4.6 surfaced a batch of render, editor, and shell issues. Tracked
      here so the spine does not advance past M4 with known regressions. Green-gate each.
  - [x] T4.7a — **Surface semantic diagnostics.** `compile()` skipped name resolution *and* the
        type checker, so unknown-name/type errors never reached the editor (a bare `gibberish`
        showed nothing). Wired both passes in (parse → resolve → typecheck → eval); stdlib lick
        names seeded as ambient so they still resolve.
  - [x] T4.7b — **Desktop squiggles render.** WKWebView (Tauri's macOS webview) ignores the
        `text-decoration: wavy` shorthand; added `-webkit-text-decoration`. Squiggles were
        web-only before. *(Same WKWebView CSS exposure applies to T7.27.)*
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

- [x] **T6.1 — Section labels (rehearsal marks).** Mark the start of a section with a label drawn
      above the staff — e.g. the A part, B part, Chorus. Language: a marker that attaches a label
      to a measure boundary. Layout: text above the staff at that measure, span-tagged for
      bidirectional mapping. (Banjo tunes are commonly split into A/B parts.) *(Done — D43:
      `section "A"` marker → `Measure.section`; reusable above-staff band, `SectionLabel` role.)*
- [x] **T6.2 — Chord symbols over bars.** Place a chord name (G, C, D7…) at the start or a beat
      within a bar so the progression sits above the tab. Language: a chord-annotation construct
      positioned at a beat. Layout: text above the staff aligned to that beat; span-tagged. *(Done —
      D44: `chord "G"` contextual-keyword marker → `Event.chord`; chord row in the above-staff band.)*
- [x] **T6.3 — Bar numbering.** Number measures above the staff. Default: number only the first
      bar of each system line. Options: number every bar; turn all numbering off. Language: a
      directive to set the mode (e.g. `barnumbers lines|all|off`). Layout: a small number above
      the chosen measures. *(Done — D45: `barnumbers lines|all|off` → `Score.bar_numbers` (default
      `lines`); top bar-number row in the above-staff band; pickups unnumbered.)*
- [x] **T6.4 — Custom (user-defined) tunings.** Beyond the builtin named tunings, let a user
      define their own per-string tuning and have it drive pitch derivation and the header tuning
      grid. Extends T2.1/T2.7 tuning resolution; needs a display name (or "Custom") for the
      header. Language: a `tuning` form taking an explicit per-string spec. *(Done — D42: inline
      `tuning [NAME] { pitch* }` with scientific-notation pitches; unnamed ⇒ no header caption.)*

**DoD M6:** section labels, chord symbols, bar numbering, and a custom tuning all parse, render
above/within the staff, and round-trip; golden snapshots + error corpora green. ✅ **Met** (D42–D45;
T6.1–T6.4 all landed, green-gated).

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
desktop / Chromium-web; uploaded/exported bundle on Firefox). *Remaining work was renumbered
2026-06-28 into one dependency-ordered sequence (T7.7–T7.34) so nothing is listed before its
blocker — see the map below.*

**Workspace shell (D41 — view registry + editor groups):**

- [x] **T7.1 — Shell foundation: view registry + editor-groups layout.** The abstraction the rest
      of the shell stands on (D41): a registry of *views* (`id`, title, icon, mount/unmount,
      serializable state), classed as **global singletons** (dock, bottom bar) or **document-bound**
      (editor, render, preview, looper). Layout = **editor groups** — panes holding stacked tabs —
      with split, move-tab-between-groups, resize, and **maximize ("zoom") a group**. Generalizes
      today's editor|render split (its N=2, one-tab-each case). No free-floating docking (deferred).
  - *Landed (incremental scope):* pure model `workspace.ts` (view registry; groups → tabs → active;
    weights; `activateTab`/`resizePair`/`toggleMaximize`) + `Workspace.svelte` chrome (tab strips,
    resize gutters, per-group maximize) mounting each active view via a Svelte snippet. `App.svelte`
    now drives the editor|render split through this model. **Deferred to when a second tab exists
    (T7.4/T7.5):** adding groups (split), moving a tab between groups, and layout serialization.
- [x] **T7.2 — Left project dock + Cmd/Ctrl-B.** *(global-singleton view)* Collapsible left dock
      showing project structure, toggled by Cmd/Ctrl-B and a bottom-bar button. The file tree comes
      from the project/import model (M5, D38) — a live folder on desktop / Chromium-web, or the
      loaded project bundle on Firefox.
  - *Landed:* `Dock.svelte` (global singleton), mounted left of the workspace in a new `.body` row,
    shown/hidden by the `dockOpen` seam (Cmd/Ctrl-B + bottom-bar toggle from T7.3). Lists the open
    project's files — entry document + bundle libs — via pure `project.ts` `projectFileList`
    (sorted, entry flagged active), headed by the bundle name. *Display-only:* opening a file as an
    editor tab is **T7.4** (needs the per-file/multi-doc machinery T7.1 deferred). *Note:* the tree
    currently sources from the loaded bundle map; the **live-folder (FSA) source isn't built yet**
    (D38, now **T7.15**) — `openProject` picks one score or one `.ctabz`, so flat lists today;
    hierarchical folder rendering + live-folder watching land with T7.15.
- [x] **T7.3 — Bottom status bar (slick, minimal, non-invasive).** *(global-singleton view)* A
      small bottom bar hosting the dock toggle and the diagnostics button (T7.28); sets the
      bottom-control styling — small, unobtrusive, out of the way. Pairs with T7.28 and T7.34.
  - *Landed:* `BottomBar.svelte` (registered `bottomBar` global-singleton in the view registry),
    rendered as fixed chrome below the workspace. Left: a dock toggle wired to `dockOpen` +
    **Cmd/Ctrl-B** (the panel it reveals is T7.2; the control + keybinding live here). Right: a live
    problem indicator ("No problems" / error+warning counts) from the compile's diagnostics, via a
    pure `diagnosticCounts` helper. Added shared `--error`/`--warning` theme tokens (light+dark) for
    cohesion with the diagnostics tooltip/panel (T7.27/T7.28). *Deferred:* making the indicator a
    button that opens the exhaustive panel + jumps to spans is **T7.28**.
- [x] **T7.4 — Editor views + multi-file tabs.** *(document-bound)* Each open `.ctab` is an editor
      view; `import`ed files open as tabs across the groups. Depends on M5 import / multi-file;
      tab/group mechanics come from T7.1. *Decomposed into T7.4a (model refactor) + T7.4b (multi-file
      UX) to separate the risky model change from the new behavior.*
  - [x] **T7.4a — Document-session model refactor (behavior-preserving).** Extracted App's
    single-document globals (content, name, path, dirty baseline, save) into a keyed session store
    `documents.ts` (`DocStore`/`DocSession` + pure `putDoc`/`setActiveContent`/`markActiveSaved`/
    `isDirty`); App now derives the active doc's `source`/`name`/`path`/`dirty` from it and routes
    open/new/save/edit through it. One session in this phase, so no visible change — green against
    the existing 25 App tests (no regression). Compile-result/selection/zoom stay global; T7.4b makes
    them per-doc.
  - [x] **T7.4b — Open files as editor tabs + dock wiring.** Each opened/imported file gets its own
    `docId`, editor tab, and render. Per-doc compile output/highlight/layout-width keyed by id (one
    latest-wins compiler each) so two files' renders coexist on the T7.5 mechanism. Open/New/dock add
    (or focus) a tab instead of replacing — so the discard-on-open guard is gone (opening never loses
    work). Active-follows-focus (editor focus + tab activation, via Editor `onFocus` + Workspace
    `onActivateView`) drives the topbar/Save/Export. Dock files open on click (`onOpenFile`); editing
    a lib syncs the project map + recompiles dependents. New `RenderView.svelte` owns each render's
    pane width + reflow; views keyed by instance so a doc switch mounts a fresh editor. *Deferred
    (documented):* **closing tabs**; **keep-alive** across stacked-tab switches (a switch remounts, so
    in-editor undo/scroll reset — side-by-side groups keep both mounted, so the common case is fine);
    **multi-project import isolation** (`projectFiles` is the current project context, replaced on
    Open); **per-doc zoom** (zoom stays global). *Fixed en route:* an `Editor` selection-effect
    re-dispatch loop (now idempotent) surfaced by the per-doc highlight wiring.
- [x] **T7.5 — Render as a document-bound view.** Make the render a document-bound view placeable
      in any group, so "file + its render" sits side by side and file A's / file B's renders can
      coexist. Resize/reposition come from the group layout (T7.1) — no bespoke docking.
  - *Landed:* turned on the move/split machinery T7.1 deferred, with the render as first consumer.
    Model `workspace.ts` gains `moveTab` (drag a tab into another group; emptied groups drop; active
    ids + a stranded maximize repaired) and `splitTab` (pop a group's active tab into a fresh group
    beside it, halving the width). `Workspace.svelte` makes tabs draggable, every group a drop
    target (with an accent drop cue), and adds a per-group **Split** button (keyboard-reachable,
    shown when a group stacks >1 tab). So the render can be dragged onto the editor's group and split
    back out — placeable in any group, resize via the existing gutters. *Multi-document coexistence
    (file A's vs file B's renders) arrives with the second document in **T7.4**;* T7.5 is the
    mechanism. *Deferred:* keyboard-driven tab move/merge (split covers separation by keyboard).
- [x] **T7.6 — Print-preview view.** *(document-bound)* The final printed (light) output regardless
      of editor theme. *Recommendation:* implement as a mode reusing the export styling (T5.3),
      **not** a separate pipeline, so it isn't duplicative of the live render.
  - *Landed:* `PreviewView.svelte` (registered `preview` document-bound view) renders the document's
    live render tree through the **export serializer** (`renderTreeToSvg`, T5.3) inline — the same
    light, self-contained SVG export produces, shown as a white sheet on a fixed light backdrop so it
    reads the same in either app theme. No second layout pipeline; it reuses the per-doc compile
    result. A topbar **Preview** button opens it as a tab beside the render (active-follows-focus via
    `onActivate`). Print-to-paper pagination is **T7.19** (PDF); this is the on-screen preview.
**Remaining work — one execution-ordered sequence (T7.7–T7.34).**

*Renumbered 2026-06-28 from a NOTES.md triage + re-order so the list is dependency-sorted (nothing
before its blocker). New tasks are from `docs/NOTES.md`; the M4 T4.7 render/UI items were folded in
and renumbered. **Old → new:** T4.7t→T7.17 · T4.7s→T7.18 · (old)T7.9→T7.19 · (old)T7.13→T7.20 ·
(old)T7.8→T7.21 · (old)T7.7→T7.22 · (old)T7.10→T7.24 · (old)T7.11→T7.25 · (old)T7.12→T7.26 ·
T4.7i→T7.27 · T4.7m→T7.28 · (old)T7.14→T7.30 · T4.7h→T7.31 · T4.7r→T7.32 · T4.7q→T7.33 · T4.7n→T7.34.*

*Bugs (broken now, no upstream deps):*

- [x] **T7.7 — Fix: group sizing after move→split→move.** The render (then the editor on tab switch)
      no longer fills its group and gets cut off after move→split→move. *(NOTES #18.)*
- [x] **T7.8 — Fix: opening a project clears the previous one.** Opening a new project left the old
      project's documents, tabs, and renders open, so a stale render lingered; a project open now
      replaces the prior one (dirty-only confirm before discarding unsaved work). *(NOTES #17.)*
- [x] **T7.9 — Fix: only panes scroll, not the page.** The app shell (`main`) must never scroll; only
      the scrollable view bodies (editor, render, preview, dock) do — a tall render grew the shell
      instead of scrolling internally. *(NOTES #4.)*

*Icon foundation → workspace UX:*

- [x] **T7.10 — Self-host Material Symbols icons (D51).** Bundle the Material Symbols set locally (font
      or SVGs in the build) so icons work fully offline on desktop — no CDN. Establish the icon-usage
      convention (a small `Icon` wrapper/class) the rest of the UI draws from. *(NOTES #1.)*
- [x] **T7.11 — Close tab.** A close affordance on each tab that removes that view instance from its
      group (dropping an emptied group, like `moveTab`), with an **unsaved-changes guard** when closing
      the last editor of a dirty document, and session cleanup when a doc has no remaining views. The
      close-tab deferred in T7.4b. *(NOTES #8.)*
- [x] **T7.12 — Group controls in the tab strip.** A tidy control set shown on the **active group**
      only: **New ("+")** (replaces the topbar New), **maximize**, **close** (T7.11), **Fit**
      (aspect-ratio icon, moved off the render toolbar), and **split** (left/right — up/down deferred,
      D50). **Remove the render zoom toolbar** (the % field goes away; zoom lives in a command/Fit).
      **Double-click a tab to maximize/restore.** Uses the T7.10 icons.
      **Also — reopen a closed render:** the active group's control set carries a render launcher (♪
      icon) when its active tab is an editor — spawns that doc's render if closed, or jumps to it if
      already open — closing the T7.11 gap where a closed render had no way back. (Preview reopening
      stays on the topbar Preview button.) *(NOTES #5, #6, #7, #10, #15.)*
- [x] **T7.13 — Drag cue: dim only the drop area.** While dragging a tab, indicate the target by
      dimming **only the drop region** (the group body) to a movable-cue colour, not outlining the
      whole field. Refines the T7.5 drop cue. *(NOTES #3.)*
- [x] **T7.14 — Iconify the topbar + styled tooltips.** Replace the remaining topbar text buttons with
      Material Symbols icons (T7.10), and give **every** control a neat CSS hover tooltip (replacing
      native `title=`), ensuring full coverage. Feeds the T7.34 cohesion pass. *(NOTES #2, #9.)*

*Project open:*

- [x] **T7.15 — Open a project as a folder (D38 live folder).** Open a whole project directory, not
      just a single score or `.ctabz` bundle: a live folder on desktop (Tauri fs) / Chromium-web (File
      System Access API); the dock then shows the real folder tree (hierarchical) and imports resolve
      against it. The live-folder source flagged unbuilt in T7.2. Pairs with T7.8. *(NOTES #16.)*
      Desktop gets the live, watched folder; web keeps the `.ctabz` bundle (FSA Chunk D skipped — D38).
- [x] **T7.15b — New = an unsaved draft listed in the dock (IDE-style).** New (the T7.12 "+") should
      create an **untitled, dirty draft** that's surfaced in the dock's file tree and saved through the
      in-app flow — not a phantom "clean" doc the user only ever names via the system save dialog. Two
      halves: **(a)** the session/dirty model marks a **never-saved** doc dirty until its first save
      (cleanly reusing the T7.11 close-guard / T7.8 open-guard), and **(b)** open-but-unsaved docs appear
      in the dock tree. **Depends on T7.15** — the dock-presence half should land against the
      dock-as-folder tree, not today's flat libs list. *(Raised during T7.12; landed with T7.15.)*

*Dock UX (placed by topic next to T7.15; not strict dep-order):*

- [x] **T7.35 — Dock folder indent guides.** Draw a vertical guide line down the left of each folder's
      children (Zed/VS Code style), terminating where that folder's contents end, so folder width and
      nesting read clearly. Pure `Dock.svelte` markup/CSS on the existing recursive `row` snippet; no
      model change.
- [x] **T7.36 — Dock file management (right-click context menu).** Right-click a dock item → a context
      menu with **New File / New Folder / Rename / Delete**. New File/Folder created in the right-clicked
      folder (or project root otherwise); Delete confirms via `ConfirmDialog` (native confirm no-ops on
      desktop). On desktop these hit the real live folder; the watcher re-scans and reconciles the dock,
      and the new file opens as a tab. Needs: empty-folder scan support (dirs in the tree), an in-app
      naming input (not `window.prompt`), new fs ops behind the io.ts seam + `fs:allow-remove/mkdir/rename`
      capabilities, and a reusable context-menu component. Desktop live-folder only (web joins with the
      deferred FSA work); naming via an inline tree input, not a modal.

*Render content & labels:*

- [x] **T7.16 — Contextual render (def-gallery) + filename tab labels (D49).** Render/preview is
      **contextual**: a file with a `score{}` renders its score; a **lib** (defs only) renders a
      **gallery** previewing each `def` on its own page. Needs **core** support to render an individual
      `def` (e.g. synthesize a minimal score per def). Tab labels become the **filename**, with the
      icon distinguishing view type (editor / render / preview). *Open sub-decision (resolve here):* how
      to render a parameterized `def` — representative/default args, nullary-only, or a placeholder.
      *(NOTES #12, #13.)* Sub-decision resolved: a parameterized `def` previews under representative
      sample args (the open melody strings 3-2-1) with a signature-card fallback (provisional). Tab close
      also became a uniform "Close tab" on **Cmd/Ctrl-W**.

*Render-layout → export track:*

- [x] **T7.17 — Justify systems + pin page width.** Pin the page to the layout target, then stretch
      short systems to fill it. **Blocks T7.19.** Relates to T3.3/T3.4 and T7.18.
  - [x] T7.17a — **Pin page width.** `overall_width(...).max(config.width)` in `layout()` so the page is
        the layout target, not content-derived — the centred header and zoom stop reflowing as measures
        are added. (The def-gallery already pins; prerequisite for justifying.)
  - [x] T7.17b — **Justify systems.** Stretch measures/events to fill each system's width within the
        pinned page, so a line holding one (or a few) measures no longer renders short.
- [x] **T7.18 — Even out intra-measure spacing.** Reviewed the trailing-space vs leading-pad asymmetry
      and **kept the onset-based model as-is by decision** (it's standard engraving — a long note earns
      space after it). No code change; see DESIGN changelog for the options weighed.
- [x] **T7.19 — Paginated PDF export (D30).** The MVP's third export format and the distribution
      standard for tab. Builds on the pinned page (T7.17) and reuses the print styling (T5.3 / preview
      T7.6). Depends on T7.17.
  - [x] T7.19a — **Pagination layout.** Fixed Letter/A4 pages, systems packed per page, margins, and a
        per-page sheet header — layout work, not a serializer. *Tests:* page-break placement
        (systems-per-page) golden cases; multi-page doc emits N pages; one-page doc emits one.
  - [x] T7.19b — **PDF emission.** Serialize the paginated tree to valid PDF bytes. *Tests:* valid PDF
        bytes (header + page count).
  - [x] T7.19c — **Save via the io seam.** Binary write on desktop, download on web.
- [x] **T7.20 — Unified export control (SVG/PNG/PDF, D48).** Fold M5's separate export buttons and the
      PDF export (T7.19) into a single **Export** button with a format picker (SVG / PNG / PDF). One
      control, one dropdown; reuses the io seam. Depends on T7.19; pairs with the cohesion pass (T7.34).
      *Already satisfied (no new code):* M5's separate SVG/PNG buttons were folded into the single
      download-icon Export dropdown during the iconify pass; **T7.19c** added PDF, completing the picker.

*Editor tooling:*

- [x] **T7.21 — Dark theme by default.** Default the app to the dark theme (keep the light / system
      toggle).
- [x] **T7.22 — Editor line numbers + gutter divider.** CodeMirror `lineNumbers()` gutter with a
      divider rule between the gutter and the code text.
- [x] **T7.23 — Editor code-folding for `{ }` blocks.** A fold control on lines opening a curly block
      (`score {`, `measure {`, `repeat {`, `def … {`): a down chevron that collapses the block's
      contents and turns into a coloured side arrow to re-expand. CodeMirror `foldGutter`/code-folding
      keyed to the brace structure. *(NOTES #14.)*
- [x] **T7.24 — Autocomplete & completion hints (toggleable, D46).** CodeMirror completions driven by
      the core's existing knowledge, Tab to accept, with a setting to toggle them on/off.
  - [x] T7.24a — **Candidate source from core.** Surface the completion lists through the `core` adapter
        from the keyword table + stdlib/`def` registry (no second source of truth).
  - [x] T7.24b — **CodeMirror completion + inline hints.** Keywords with a fixed value set hint their
        options (`instrument` → `banjo`/`guitar`; `tuning` → named tunings; `barnumbers` →
        `lines`/`all`/`off`), top-level keywords hint their operand (`title` → `"Title"`), and
        stdlib/`def` names complete as identifiers. Tab to accept.
  - [x] T7.24c — **On/off setting.** A toggle for autocomplete + inline hinting.
- [x] **T7.25 — DSL formatter (button + format-on-save toggle, D47).** A canonical pretty-printer for
      `.ctab`, exposed as a toolbar action and an on-save option.
  - [x] T7.25a — **Core `format(source) -> String`.** Over the parsed AST/token stream — deterministic,
        idempotent, comment-preserving; a document with parse errors is returned untouched. *Tests:*
        idempotence (`fmt(fmt(x)) == fmt(x)`); a messy→canonical golden corpus; comments survive.
        *(Provisional canonical style: 2-space indent, attached marks/durations, single-space metadata
        — no column-alignment yet; revisit in the UI polish pass.)*
  - [x] T7.25b — **Format button + format-on-save toggle.** Bottom-bar **Format** action plus a
        **format-on-save** setting, both calling the core formatter via the wasm/Tauri seam. Format
        replaces the buffer in one **undoable** transaction.
- [x] **T7.26 — Theme switcher in the bottom bar.** Move the light / dark / system control out of the
      topbar into the bottom status bar (T7.3) as a compact control. Folds into T7.21's toggle and the
      T7.3 bottom-bar styling. *(Joins the bottom-right control cluster: theme · format-on-save ·
      autocomplete · diagnostics.)*

*Diagnostics:*

- [x] **T7.27 — Diagnostic tooltip readability.** Currently white-on-white until selected; give it
      themed background/foreground/border keyed to the semantic tokens (WKWebView caveat — prefer
      `-webkit-` prefixes / pointer events where needed).
- [x] **T7.28 — Diagnostics panel + bottom button.** Make the bottom-bar problem indicator (T7.3) a
      button that opens an exhaustive warning/error list; clicking an entry jumps the editor selection
      to its span. *(The "error diagnostic button down below" from the notes.)*

*Help & desktop:*

- [x] **T7.29 — Help view.** A **help** button in the bottom bar opens a how-to-use-the-app tab (a
      global-singleton view with getting-started content: syntax basics, shortcuts, the workspace).
      Overlaps M8's T8.3 content. *(NOTES #11.)*
- [x] **T7.30 — Native desktop menu bar (Tauri, D48).** Wire the desktop app's native top-bar menu so
      every in-app command is reachable there, grouped conventionally — **File ▸ Open / Save / Export…**,
      **View ▸ Zoom / Reset**, **Edit** basics. Menu items dispatch the same commands as the in-app
      controls (single command source). Desktop-only (no-op on web).

*Cohesion & finishing pass — second NOTES.md triage (2026-06-29; see DESIGN changelog). All M7.*
*Recommended order: quick-win/bug sweep (T7.38–T7.40) → cohesion pass (T7.34a–g, T7.37) → functional*
*redesigns (T7.41–T7.43) → notation (T7.44).*

*Quick-win + bug sweep (do first):*

- [x] **T7.38 — Dock open by default.** The project dock starts expanded on first load. *(NOTES #9.)*
- [x] **T7.39 — Edited-dot left of the filename.** Move the unsaved-changes dot from the tab corner to
      just left of the filename, where it actually reads. *(NOTES #8.)*
- [x] **T7.40 — Bottom-bar tooltip clipping.** Tooltips anchored on the bottom bar open downward and fall
      off-screen; make the `tooltip` action edge-aware so they flip upward. *(NOTES #10.)*

*Cohesion pass — the Zed-feel polish:*

- [x] **T7.31 — Syntax-highlighting palette.** Muted two-tone palette (desaturated blue structure, warm
      tan numbers, muted green strings, gray italic comments); the full-width active-line bar became a
      faint left-edge tick. *(Done — see DESIGN changelog.)*
- [x] **T7.32 — Bidirectional highlight treatment.** The active cursor↔primitive highlight no longer
      reuses the orange `--accent`: the note keeps its ink on a soft `--select` chip. *(Done.)*
- [ ] ~~**T7.33 — Structural bidirectional mapping.**~~ **Dropped 2026-06-29** — built end-to-end (spans
      on repeat barlines / voltas / `measure {}` boxes + the chip/stroke highlight) then walked back; it
      didn't feel right. T7.32's note-chip stays the bidirectional treatment. *(See DESIGN changelog.)*
- [ ] **T7.34 — Accent/detail cohesion pass.** Umbrella for the whole-UI Zed-feel pass across topbar,
      toolbar, gutter, panels, dock / tabs / bottom bar, the iconified controls + tooltips (T7.10/T7.14),
      and the open/save/export controls. The subtasks sequence it foundation-first; sampled targets in
      the DESIGN changelog (elevation stack, offset-white text).
  - [x] T7.34a — **Palette & elevation foundation.** The bedrock everything reads against: the
        chrome→dock→editor elevation stack (sampled `#313337`→`#1F2126`→`#0E1015`), offset-white text in
        editor *and* render (not true white), and thinner dividers. *(NOTES #14. Done — see DESIGN
        changelog; warm offset-white `#bfbdb6` + warm muted `#8a8986`, tab bar at panel level.)*
  - [x] T7.34b — **Tab strip restyle.** Drop the orange top accent bar; inactive tabs a shade lighter,
        the active tab darker (Zed-style). *(NOTES #12. Done — folded into T7.34a: active tab drops to the
        editor surface `#0E1015`, accent bar dropped, inactive tabs flush at panel with muted labels.)*
  - [x] T7.34c — **Topbar declutter.** Remove the redundant "cadtab" + open-filename line (the tab strip
        already names the file). *(NOTES #13. Done — `.brand` block + `dirty` derived removed, topbar
        right-aligned; App.test.ts migrated from `.doc-name` to active-tab observables. Removing the brand
        surfaced the "what now holds Save/Export/Preview" question → see T7.45.)*
  - [x] T7.34d — **Popup de-glow.** Remove the glow/drop-shadow backgrounds on popups (diagnostics,
        new-file, export menu, completion) to fit the flat, layered look. *(NOTES #4. Done — see DESIGN
        changelog; warm-fg glow → shared `--shadow-popup` (#000-based), all popups onto elevated
        `--bg-chrome`. User picked the elevated surface against the rsvg mockups.)*
  - [ ] T7.34e — **Editor selection/highlight colour.** The CodeMirror selection reads as a harsh white
        that clashes; re-key it to the theme. *(NOTES #15.)*
  - [ ] T7.34f — **Tooltip overhaul.** Bold title + optional smaller description + the element's keyboard
        shortcut when it has one; better font + themed CSS. (T7.40 fixes clipping; this is structure +
        style.) *(NOTES #2, #3.)*
  - [ ] T7.34g — **Inline operand hints as ghost text.** (was T7.34a) Refine T7.24's operand hints
        (`title → "Title"`) from a *popup* snippet to dimmed in-buffer ghost text at a muted shade (a
        ViewPlugin ghost-text decoration), designed with the editor cohesion so the shade reads right.
- [ ] **T7.37 — Unify the render painter's role styling.** Make `Tab.svelte` consume `tabStyle.ts`'s
      shared `textAnchor()`/`isMuted()` (which `svg.ts`/export use) instead of its drifted `data-role`
      CSS (`sectionLabel`/`barNumber` differ), so screen and export never diverge. Pairs with T7.34 (the
      render text colour from T7.34a lands here too).

*Functional / workspace redesigns (M7):*

- [ ] **T7.41 — Maximize behavior.** Maximizing a tab shouldn't fully hide the others — keep the tab
      strips visible (VSCode: the maximized pane takes most space) or collapse the dock with a sliver of
      background showing (Zed). *(NOTES #1; revisits T7.7-era maximize.)*
- [ ] **T7.42 — Zoom scope rescope + render zoom bug.** Walk back the earlier per-view zoom: zoom in/out
      applies to *all* editors globally, but a render view zooms only its own tab. Fold in the render
      zoom/reformat bug (repro pending from user). *(NOTES #5, #6; design walk-back.)*
- [x] **T7.43 — Preview control → tab group.** Move the preview control into the tab-group buttons; offer
      preview when the active doc is recognized as a score. *(NOTES #11. **Coupled to / prerequisite of
      T7.45:** Preview must leave the topbar via the tab group before the desktop topbar is dropped, or
      desktop loses Preview access in the gap.)* **Done — see DESIGN changelog; the gate (`previewable`)
      offers preview for any doc that renders systems — scores *and* def-libraries — not strictly scores
      (user tried it on a defs-only lib and liked it); hidden only on empty/error docs.*
- [x] **T7.45 — Desktop chrome: custom Zed-style titlebar + web-only topbar.** Decided 2026-06-30 after
      T7.34c emptied the topbar. **Context:** Save/Export/Open already live in the desktop **native menu**
      (T7.30, `menu.ts`/`setAsAppMenu`) + ⌘S/⌘O, so the topbar icons are redundant on desktop; the web has
      no native menu and genuinely needs an in-app home for them. **Decision (user picked the ambitious
      option):** build a **custom in-window titlebar on desktop** (window `decorations` off / macOS
      `titleBarStyle` overlay), Zed-style — brand/breadcrumb on the left, **Save + Export (dropdown)** on
      the right — and make the **existing topbar web-only** (keep Open/Save/Export there, unchanged). Drop
      the in-app topbar entirely on desktop. **Preview leaves both** via the tab group (T7.43, do first).
      **Open decisions to resolve before/while building (lock with a mockup, per our rhythm):**
      (a) left content — brand `cadtab` only, vs `cadtab — <project/folder>` when one is open (NOT the
          filename — that's the tab's job, avoid re-introducing T7.34c's redundancy), vs empty;
      (b) right controls — text labels (`Save`  `Export ▾`, Zed-style) vs icons;
      (c) platform scope — macOS-first (overlay titlebar, keep native traffic lights, inset via
          `trafficLightPosition`); Windows/Linux need custom min/max/close buttons + drag regions (bigger
          lift) — decide whether to ship mac-first and keep standard decorations on Win/Linux for now.
      **Touches:** `tauri.conf.json` window config (+ maybe `src-tauri` window setup), a new titlebar
      Svelte component (drag region via `data-tauri-drag-region`, platform-aware window controls), reuse of
      the existing Save/Export handlers + the native menu. Grounding read pending (Tauri version, current
      `tauri.conf.json`, the macOS detection `menu.ts` already does via `navigator.userAgent`).
      **Done — see DESIGN changelog.** Sub-decisions locked via mockup: (a) `cadtab — <project>` breadcrumb
      (brand alone when no project); (b) text controls; (c) all platforms. Built `Titlebar.svelte` (Overlay
      titlebar + native traffic lights on macOS, `decorations:false` + custom min/maximize/close on
      Win/Linux); topbar made web-only. **Refined in review:** Save dropped from the titlebar entirely
      (⌘S + native menu suffice) — right side is just `Export ▾`. **Win/Linux hardware-verify items:**
      whether the native menu *bar* renders with decorations off, traffic-light alignment, drag, caption
      buttons — all best eyeballed on real hardware; verified live on macOS via `just dev`.

*Notation:*

- [ ] **T7.44 — Right-hand finger marks above the staff.** Move the T/I/M (thumb/index/middle) right-hand
      finger marks from below the staff into the above-staff band, and re-space the band (section / chord
      / bar-number / volta / fingers) so it doesn't crowd. *(NOTES #7.)*

**DoD M7:** the Zed-style shell (dock, tabs, bottom bar, dockable render, preview) works on desktop
+ web; justified systems with a fixed page; **paginated PDF export (T7.19)** behind a unified export
control; autocomplete, formatter, and a native desktop menu; readable diagnostics + panel; and a
cohesive, polished dark-by-default UI matching the Zed-feel target (elevation, accent restraint,
restyled tabs, decluttered topbar, themed tooltips/popups). Green.

---

## M8 — Hardening & MVP polish (ship)

**Goal:** a polished, packaged MVP on desktop + web.

- [ ] **T8.1 — Diagnostic quality pass.** Player-facing wording + `help` text for common errors.
- [ ] **T8.2 — Performance.** Debounce tuning; profile large docs; fix hotspots (revisit D21 only
      if needed).
- [ ] **T8.3 — Content.** Sample songs, broaden the stdlib, a short tutorial/getting-started doc.
- [ ] **T8.4 — Packaging.** Tauri desktop bundles (mac/win/linux) + deployed web build; release CI.
- [ ] **T8.5 — E2E smoke test** (Playwright/WebDriver) of the core flow.
- [ ] **T8.6 — Marketing & downloads website.** A static site that hosts the desktop installers
      (macOS / Windows / Linux from T8.4), shows examples and documentation, and links to the hosted
      web UI. One of the last tasks — it publishes what T8.4 packages and what M0–M7 document.

**DoD / MVP ship:** author `.ctab` banjo tab live (highlighting, diagnostics, rhythm via
stems+beams, licks, repeats, pickups, metadata), bidirectional mapping, open/save, export
SVG+PNG+PDF (paginated, T7.19) — on desktop and web. CI green; packaged. **Static site (T8.6)** hosts
downloads, examples, docs, and links to the web UI.

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
