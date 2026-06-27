# cadtab — MVP Task Order

> The sequenced build plan for the MVP (Pillar A: DSL → tab renderer). Design decisions it
> implements live in [`DESIGN.md`](./DESIGN.md) (referenced as D#). Built **walking-skeleton
> first**: the thinnest end-to-end slice runs early, then each layer thickens behind a working
> contract.

## How to use this doc

- Work milestones **M0 → M6** in order. The spine **M0 → M1 → M2 → M3** is the critical path.
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
  - [ ] T4.7f — **Strip per-line tuning.** Tuning shows at the top *and* repeats at each system
        line start; keep it only at the top.
  - [ ] T4.7g — **Header layout.** Replace the single stacked column with title — composer on
        top and a compact inline details row (`♩=tempo · instrument · tuning · capo`).
  - [ ] T4.7h — **Syntax highlighting.** Muted two-tone palette (desaturated blue structure,
        warm tan numbers, muted green strings, gray italic comments); replace the dramatic
        full-width active-line bar with a faint left-edge tick so the cursor stays readable.
  - [ ] T4.7i — **Diagnostic tooltip readability.** Currently white-on-white until selected;
        give it themed background/foreground/border keyed to the semantic tokens.
  - [ ] T4.7j — **Tab key indents.** Tab moves focus out of the editor instead of inserting
        indentation; add `indentWithTab`.
  - [ ] T4.7k — **Keyboard zoom.** Bind Cmd/Ctrl +/- (and Cmd/Ctrl 0 to fit) to the existing
        zoom controls; preventDefault to override native zoom.
  - [ ] T4.7l — **Launch desktop maximized** (currently a small 800×600 window).
  - [ ] T4.7m — **Diagnostics panel.** A warning/error count button at the bottom of the editor
        that opens an exhaustive list; clicking an entry jumps the editor selection to its span.
  - [ ] T4.7n — **Zed-inspired accent polish.** A coherent accent/detail pass across topbar,
        toolbar, gutter, and panels; fold in with T4.7g–i so the UI reads as one pass.
  - [x] T4.7o — **Secondary beams for 16ths/32nds.** A beamed group now draws a primary beam
        plus a beam per higher level over each maximal run of `flag_count ≥ level` (stacked
        above the primary, since stems point down), with partial-beam stubs for isolated values.
        *(Known edge: a 32nd on a length-clamped 5th-string stem may not reach its 3rd beam —
        rare; revisit if 32nds get real use.)*
  - [x] T4.7p — **Dense-rhythm crowding.** Event spacing is time-proportional but now floored at
        `MIN_EVENT_GAP` (0.9, just under an eighth's 1.0), so 16th/32nd runs no longer pack their
        fret numbers together while eighths-and-longer keep proportional spacing.
  - [ ] T4.7q — **Bidirectional mapping for structural elements.** Cursor↔render mapping (T4.5)
        only covers span-bearing text/notes; repeat barlines, ending (volta) brackets, and
        `measure {}` boxes don't light up. Thread spans onto those render primitives and add a
        highlight treatment for non-text primitives (lines/paths/box outlines) so clicking or
        cursoring a repeat / ending / measure highlights it. *(Extends T4.5.)*
  - [ ] T4.7r — **Bidirectional highlight treatment.** The active cursor↔primitive highlight
        reuses the orange `--accent` and reads wrong against the rest of the UI. Pick a calmer
        highlight colour/treatment, decided alongside the look-and-feel pass (T4.7h/n); touches
        the theme accent token + `.active` styles in `Tab.svelte`.
  - [ ] T4.7s — **Even out intra-measure spacing.** A bar's last note gets trailing space equal
        to its full duration, so it reads as having noticeably more room on its right than the
        small leading pad — uneven on a fully-filled measure. Do a spacing pass (revisit
        trailing-space vs leading-pad symmetry / even distribution). Pairs with T4.7t.
  - [ ] T4.7t — **Justify systems to full width.** A line holding only one (or a few) measures
        renders at its natural width, leaving the system short. Stretch measures/events to fill
        the system line width (justified systems) so even a single bar fills the line, padding
        with empty space as needed. Layout justification pass; relates to T3.3/T3.4 and T4.7s.
  - *Parked:* the showcase still emits 3 under-full-bar warnings on inherently-partial demo
    blocks (two voltas + the explicit `measure {}` fragment). Whether voltas / explicit measures
    should trigger under-full diagnostics at all is a diagnostics-quality question → revisit in
    T6.1 (and showcase metric cleanup in T6.3).
  - **Resume guide — pending T4.7 (cold-start order & pointers).** Letters a–t are stable IDs
    (assigned by discovery order), not priority; this is the suggested sequence so shared files
    and decisions are touched once. Every pending item is resume-able from its text; one *open
    decision* is flagged.
    1. **Editor quick wins** — j, k, l. Independent, no decisions; good warm-up.
       `app/src/lib/Editor.svelte` (j: `indentWithTab` in the keymap), `app/src/App.svelte`
       (k: global keydown → existing `zoomIn`/`zoomOut`/`zoomFit`, `preventDefault`),
       `src-tauri/tauri.conf.json` (l: window size / maximized).
    2. **Header & top region (core + painter)** — f, g. `crates/cadtab-core/src/layout.rs`:
       strip the per-line `StringLabel` prims in `build_system` (f — the top `build_header`
       keeps its `Tuning` text, confirmed, so tuning isn't lost); rework `build_header` for the
       title—composer line + inline details row (g). New text roles get styling in
       `app/src/lib/Tab.svelte` `TEXT_STYLE`.
    3. **Horizontal spacing (core)** — s, t. One `layout.rs` pass: `plan_measure` trailing space
       (s) and `build_system`/`pack_systems`/`overall_width` justification (t). Snapshot-heavy.
    4. **Diagnostics UI (frontend)** — i, m. `app/src/lib/diagnostics.ts` (i: themed tooltip
       bg/fg — same WKWebView caveat as T4.7b); new component + `App.svelte` (m: count button +
       panel wired to `result.diagnostics`, click-to-jump via existing span→selection path).
    5. **Visual feel pass (frontend)** — h, r, n. The cohesive colour/accent pass.
       `app/src/lib/highlight.ts` + `Editor.svelte` (h), `app/src/app.css` `--accent` +
       `Tab.svelte` `.active` (r), broad CSS (n). **DECISION PENDING (T4.7r):** the
       bidirectional-highlight treatment is unchosen — pick it (e.g. desaturated fill vs
       underline vs halo, and which token) via a quick question when this cluster starts; T4.7q
       reuses whatever is chosen. `n` is the umbrella — h, i, g, r feed it; treat it as "make the
       rest cohere," not a separate chunk.
    6. **Structural bidirectional mapping (core + frontend)** — q. Do after 5 so the non-text
       highlight reuses T4.7r's treatment. Core: thread spans onto repeat-barline / volta-bracket
       / `measure {}`-box prims in `layout.rs`. Frontend: extend `Tab.svelte` `.active` beyond
       `text`/`path` to line/box prims.

**DoD M4:** the live editor works end to end on desktop + web; component/integration tests green.

---

## M5 — Persistence & export

**Goal:** open/save and share (D29, D30, D38).

- [ ] **T5.1 — Open/Save `.ctab`.** Desktop fs dialogs; web File System Access API /
      download-upload (D38).
  - *Tests:* save→open round-trip.
- [ ] **T5.2 — `import` resolution.** Desktop multi-file; web stdlib-only (D38).
- [ ] **T5.3 — Export SVG + PNG (D30).** Render tree → SVG string → PNG raster.
  - *Tests:* export emits valid SVG; PNG non-empty.
- [ ] **T5.4 — New-from-template / recent files** (nice-to-have; sub-task if time-boxed).

**DoD M5:** round-trip persistence + SVG/PNG export; green.

---

## M6 — Hardening & MVP polish (ship)

**Goal:** a polished, packaged MVP on desktop + web.

- [ ] **T6.1 — Diagnostic quality pass.** Player-facing wording + `help` text for common errors.
- [ ] **T6.2 — Performance.** Debounce tuning; profile large docs; fix hotspots (revisit D21 only
      if needed).
- [ ] **T6.3 — Content.** Sample songs, broaden the stdlib, a short tutorial/getting-started doc.
- [ ] **T6.4 — Packaging.** Tauri desktop bundles (mac/win/linux) + deployed web build; release CI.
- [ ] **T6.5 — E2E smoke test** (Playwright/WebDriver) of the core flow.

**DoD / MVP ship:** author `.ctab` banjo tab live (highlighting, diagnostics, rhythm via
stems+beams, licks, repeats, pickups, metadata), bidirectional mapping, open/save, export
SVG+PNG — on desktop and web. CI green; packaged.

---

## Critical path & parallelism

- **Spine (sequential):** M0 → M1 → M2 → M3. Each strictly needs the prior.
- **M4** needs M3 (render tree) + M0 (shell). **M5/M6** follow M4.
- **Parallelizable within milestones:** lexer (T1.2) vs AST types (T1.3); instrument table (T2.1)
  vs stdlib (T2.7); painter primitive kinds (T4.2) across types.

## Risk register

| Risk | Where | Mitigation |
|------|-------|------------|
| Beam/stem geometry is fiddly | T3.5 | Sub-task hard; rhythm snapshot matrix; isolate as pure fn |
| Resilient parser recovery quality | T1.4 | Error-recovery corpus from day one; AST boundary keeps parser swappable (D18) |
| Bidirectional mapping correctness | T4.5 | Spans threaded from M1 (D20); dedicated lookup tests |
| WASM/Tauri parity drift | T0.8 | Both backends in CI from M0; one shared `core.compile` contract |
| Layout reflow performance | T3.4/T6.2 | Profile; debounce; full-recompile is fine until proven otherwise (D21) |
