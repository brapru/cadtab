# cadtab — Design & Architecture

> Living document. Captures major design and architectural decisions, with rationale,
> so they can be referenced and revisited later. Newest decisions append to the log
> at the bottom; the top sections reflect current intent.

## 1. Vision

**Music tab as code.** A small, OpenSCAD-inspired domain-specific language for describing
music textually that compiles and renders to instrument **tablature**. Banjo-first, but the
underlying model is instrument-agnostic (guitar, mandolin, etc. fall out of the same design).

Two pillars:
- **A. DSL → tab renderer** (the MVP) — write music as code, see rendered tab live.
- **B. Practice tool** (fast-follow, post-MVP) — import YouTube/mp3/mp4 with focused
  playback: pitch-correct slow-down and A/B phrase looping for drilling.

Secondary goal: a high-quality, enjoyable vehicle for getting strong at Rust, leveraging
existing compiler expertise.

## 2. Locked decisions (Phase 1)

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | **MVP scope = Pillar A only** (DSL + tab renderer). Practice tool is a deliberate fast-follow. | Smallest coherent, maximally differentiated scope; aligns with compiler strengths. |
| D2 | **DSL ambition for MVP = "Notes + named licks."** Linear note/measure/tuning entry plus reusable, parameterizable named licks/patterns (variables + functions that expand to note sequences). Full OpenSCAD-style functional transforms are a post-MVP direction. | Genuinely useful and exercises a real lexer/parser/AST without over-scoping language semantics. |
| D3 | **UI stack = Tauri 2.** Rust core (compiler **and** layout engine) behind a thin TS/SVG frontend. | Best path to a slick, modern, reactive UI; SVG is ideal for notation; web platform makes Pillar B (playback) dramatically easier later. |
| D4 | **Web target via WASM.** Same TS frontend; compile the Rust core to WASM for the browser, native lib for the Tauri desktop build. | "Desktop + web from one core" with one frontend; stronger than alternatives' web story. |

### Rejected alternatives (so we don't relitigate)
- **Iced / egui (pure-Rust UI):** lower "slick" ceiling, and Pillar B (pitch-correct
  slow-down, video, YouTube) becomes a large native media-engineering effort. The web
  platform gives `playbackRate` + `preservesPitch`, `<video>`, and the YouTube IFrame API
  largely for free. Iced remains the fallback *only if* maximizing total Rust surface area
  becomes a primary goal.

## 3. Core architectural principle: model / view separation

Mirrors CAD's split between the parametric **model** and the rendered **view**. The DSL
compiles to an abstract **musical model**; the renderer is a *pure function of the model*.
This is what makes "any instrument" and (later) "transformations on phrases" tractable.

```
                          ┌──────────────────────── Rust core ────────────────────────┐
  source text  ──▶  lex ──▶ parse ──▶ AST ──▶ resolve/eval ──▶ musical model ──▶ layout ──▶ render tree
                                              (licks, vars)     (instrument-          (positioned
                                                                 agnostic IR)          primitives)
                          └────────────────────────────────────────────────────────────┘
                                                                                         │
                                                                          serializable (serde)
                                                                                         │
                                              ┌──────────────── TS frontend ─────────────┘
                                              │
                              render tree ──▶ SVG painter (thin)  +  editor / chrome / interaction
```

Key consequences:
- The **layout engine lives in Rust**, not TS. The frontend is a thin painter of positioned
  primitives. Keeps the interesting, deterministic, testable work in Rust.
- The pipeline `source text → render tree` is a **pure, UI-free function** → unit-testable
  end to end with zero UI.
- One render tree drives: in-app SVG, future PDF/SVG/PNG export, and the WASM web build.

## 4. Phase 2 status

All Phase 2 architecture topics are resolved — live status in §7b; resolved detail in §5–§11,
with review refinements in §11b. Phase 3 (MVP task order) lives in `docs/TASKS.md`.

## 5. Musical model (IR) & instruments — resolved

**Position-canonical, pitch-derived.** A note's truth is `(string, fret)` — what tab *is*
and what the player controls. Pitch is computed (`open_pitch[string] + fret`), never authored.
Transposition / future transforms / future audio re-emit positions when explicitly asked.

```rust
struct Pitch(i16);                        // MIDI-style semitone; tab needs no enharmonic spelling
struct Duration { num: u32, den: u32 }    // fraction of a whole note; rational ⇒ exact tuplets/dots
struct TimeSig  { num: u8, den: u8 }
struct Position { string: u8, fret: u8 }  // string: 1-based, 1 = highest pitch (D37)

struct Note {
    pos: Position,
    dur: Duration,
    right_hand: Option<RightHand>,        // optional right-hand execution mark
    technique: Option<Technique>,         // set only by DSL technique functions (never a raw field)
    tie: bool,                            // ties into the next note (D36)
}
enum RightHand { Finger(Finger),          // Thumb | Index | Middle  (banjo / fingerstyle)
                 Strum(Dir) }             // Up | Down               (guitar / strum)
enum Technique { HammerOn, PullOff, SlideTo, Bend, Choke, Ghost }   // h/p/sl/b… render marks

struct ChordNote { pos: Position, right_hand: Option<RightHand> }   // a pinch member
enum Event {
    Note(Note),
    Chord { dur: Duration, notes: Vec<ChordNote> },   // ONE shared duration (D39); pinch / stacked
    Rest(Duration),
}
struct Phrase  { events: Vec<Event> }      // unit that licks produce & (later) transforms operate on
struct Measure {
    events: Vec<Event>,
    meter: Option<TimeSig>,                // meter change at this bar, if any
    repeat_start: bool, repeat_end: bool,  // bounds of a `repeat { }` section (D32)
    ending: Option<u8>,                    // volta number, if inside an `ending(n)` block (D32)
    is_pickup: bool,                       // partial bar, excluded from barring check (D33)
}
struct ScoreMeta { title: Option<String>, composer: Option<String>, tempo: Option<u16> }  // D34
struct Score {
    meta: ScoreMeta,
    instrument: Instrument,                // builtin + tuning override (D35)
    capo: Vec<String>,                     // display-only header labels (D6)
    measures: Vec<Measure>,
}

struct StringDef  { open_pitch: Pitch, label: String }    // Vec index 0 = string 1 (D37)
struct Instrument { name: String, strings: Vec<StringDef> }
```

Decisions:
- **D5 — Position-canonical, pitch-derived.** (Rejected: pitch-canonical + fret-assignment
  solver — a research problem and worse authoring experience.)
- **D6 — Minimal instrument plumbing.** Instrument = ordered strings (label + open pitch).
  No `min_fret`, no per-string capo machinery. **Capo is display-only metadata for the MVP**
  (revisit when audio lands, since that's the only place it's numerically load-bearing).
- **D7 — Unified right-hand mark** (optional, per note): `Finger(T|I|M)` or `Strum(Up|Down)`.
  Required MVP feature; keeps the "any instrument" promise. Banjo T/I/M rendering is a
  deliberate differentiator.
- **D8 — Techniques are DSL functions over the model.** `hammer/pull/slide/bend/choke/ghost`
  are surface functions that *lower* to the `Technique` annotation so the renderer can draw
  the mark. Connecting techniques (hammer/pull/slide) annotate the *target* note; single-note
  ones (bend/choke/ghost) annotate that note. The raw field is never authored directly.
- **D9 — Rhythm as rationals; chords in, multi-voice out for MVP.** Pinches/chords supported;
  multi-voice deferred but not precluded (room for a `voice` on events/measures later).
- **Review refinements (D32–D39, §11b)** adjust this model: chord = one shared duration +
  per-note finger; `tie` flag on notes; rest literal; `ScoreMeta`; measure repeat/ending/pickup flags.

## 6. DSL surface syntax — resolved

```
title    "Syntax Showcase"
composer "cadtab"
tempo    130                // metadata → sheet header (D34)

instrument banjo            // builtin: 5 strings, Open G default
tuning     openG            // override open-string pitches (D35)
capo "5th string @ 2"       // display-only header note (D6)

score {
  time 4/4
  default 1/8               // baseline duration for unmarked notes

  pickup { 2:0.i 1:0.t }    // partial bar, excluded from barring check (D33)

  repeat {                  // musical repeat section (D32); endings nest inside
    3:0.t  2:0.i  1:0.m   5:0.t  3:0.i  1:0.m   // body (auto-barred per `time`)
    [1:0.m 5:0.t]_4         // pinch: one shared duration (D39), quarter
    ending(1) { r_8  3:2 ~ 3:2 }   // 1st pass — rest then a tie (D36)
    ending(2) { 1:0.m 1:0.m }      // 2nd pass
  }

  loop 2 { 3:2 3:4 }        // unroll loop (D32): writes the body out 2×

  measure {                 // explicit measure override when you want it
    hammer(3:0, 3:2)        // technique = function (D8)
    bend(1:7)
  }
}
```

- **D10 — Note literal `string:fret`** + optional right-hand mark suffix `.t/.i/.m`
  (`.d`/`.u` for strum down/up). Events are juxtaposed; whitespace incl. newlines separates
  only ⇒ free formatting.
- **D11 — Durations: `default` baseline + one-shot `_N`.** `_N` suffix (N = denominator; `.` =
  dotted; tuplet marker TBD). An omitted duration uses the current `default` — a cascading
  directive (like `time`) that may recur mid-score to move the baseline. An explicit `_N`
  overrides exactly its own note and never threads forward. Common case (a constant subdivision
  set once via `default`) = zero per-note typing; a run of another value = change `default`
  before it. *(Revised from the original Lilypond-style sticky model: a stray `_N` silently
  rewriting every following note competed with `default` and surprised in practice.)*
- **D12 — Auto-barring** from `time`, with explicit `measure { }` as override. The compiler
  owns barline insertion and **diagnoses over/under-full bars**.
- **D13 — Program shape:** top-level declarations (`instrument`/`tuning`/`capo`) + `score { }`
  with block-scoped cascading settings (`time`, `default`) and optional `measure { }` blocks;
  chords/pinches in `[ ]`; techniques as functions.

## 7. Licks & functions — resolved

```
def forward_roll(chord) {                 // functions take/return Phrase values
  chord.0 .t   chord.1 .i   chord.2 .m    // phrase indexing; reapply right-hand + timing
}
let g_chord = [3:0 2:0 1:0]
score {
  default 1/8
  forward_roll(g_chord)                   // call splices the returned phrase
  loop 3 { forward_roll(g_chord) }        // unroll loop: 3 written-out copies (D32)
  forward_roll(...g_chord)                // spread: splat a phrase into positional args
}
```

- **D14 — Functions evaluate to `Phrase` values, not textual macros.** A `def` body is
  evaluated to a phrase; a call splices it at the call site. Lexical scope ⇒ no macro-hygiene
  problem. This is the exact seam the post-MVP transform layer plugs into.
- **D15 — Minimal static types.** Small value taxonomy: `Int`, `Duration`, `Position`,
  `Note`, `Phrase`. Light static checking (arity + value kind), *not* full HM. Chosen to power
  crisp, instant in-editor diagnostics (squiggles) for the live-recompile UX.
- **D16 — Phrases-as-params; `loop N { }` unroll builtin; shipped stdlib of licks.** Canonical
  rolls (forward, backward, alternating-thumb, Foggy Mountain) ship as overridable `def`s;
  users can `import` their own library files. (Library *storage format* deferred to Persistence.)
  *(Loop renamed `repeat N` → `loop N` per D32; `repeat` is now the musical repeat.)*
- **D17 — Phrase indexing AND spread.** `phrase.N` + `len(phrase)` (general primitive) and
  `...phrase` spread/splat (positional-arg sugar). Note: "roll over a chord" re-times
  simultaneous notes into a sequence — a *baby transform*, foreshadowing the post-MVP layer.

## 7b. Open for Phase 2 (remaining)

- [x] Compiler pipeline internals — resolved, see §8.
- [x] Render-tree contract — resolved, see §9.
- [x] Layout engine — resolved, see §9.
- [x] Workspace / crate structure — resolved, see §11.
- [x] Frontend architecture — resolved, see §10.
- [x] Diagnostics surfacing — resolved, see §11.
- [x] Persistence — resolved, see §11.

**Phase 2 (architecture) complete.** Phase 3 (MVP task order) in §12.

## 8. Compiler pipeline — resolved

```
source → lex → parse → AST → resolve (defs/lets/imports) → typecheck (minimal, D15) → eval → musical model
```

- **D18 — Hand-rolled recursive descent** (+ Pratt for expression precedence). Chumsky was a
  viable alternative (familiar, built-in recovery), but the grammar is modest and fine-grained
  control over error recovery is worth it. The clean `source → AST` boundary keeps the parser
  isolated and swappable — nothing downstream depends on how the AST is built.
- **D19 — Resilient / error-tolerant parsing.** Never bail on first error: recover, emit a
  *partial* AST, and report *multiple* diagnostics. A half-typed document still renders its
  valid parts. (Required by the live, debounced recompile UX.)
- **D20 — Spans threaded through the ENTIRE pipeline** (AST → model → render tree), enabling
  **bidirectional source↔render mapping as an MVP feature**: click a note in the tab → cursor
  jumps to the source that produced it, and vice versa. Architectural commitment across every
  layer — cheap if planned now, miserable to retrofit.
- **D21 — Full recompile per debounced change** for MVP (no salsa-style incrementality). Docs
  are small and the pipeline is fast; revisit only if profiling demands it.

## 9. Layout engine & render-tree contract — resolved

`fn layout(model, LayoutConfig { width, … }) -> RenderTree`. Pure function; the TS side paints
the result verbatim (no layout logic in TS).

```rust
struct RenderTree { meta: LayoutMeta, header: Vec<Primitive>, systems: Vec<System> }
struct System     { bounds: Rect, measures: Vec<MeasureBox> }
struct MeasureBox { bounds: Rect, prims: Vec<Primitive>, span: Span }   // span ⇒ bidi mapping
enum Primitive {
    Line { x1,y1,x2,y2, weight },        // string lines, barlines, stems, beams
    Text { x,y, content, role, span },   // fret numbers, T/I/M, strum arrows, labels
    Path { cmds, span },                 // slurs, ties, bends, choke arcs
}
```

- **D22 — Render tree is lightly hierarchical** (`System → MeasureBox → Primitive`), with
  **logical coordinates** (1 unit = string spacing) scaled by the frontend via SVG `viewBox`
  (free zoom, crisp at any DPI). Serialized with **serde → JSON** across the IPC/WASM boundary.
  Span-bearing nodes carry source spans (D20).
- **D23 — Layout is parameterized by target width.** Screen passes viewport width ⇒
  **responsive reflow** on debounced resize; PDF export passes a fixed page width. One engine,
  different `LayoutConfig`.
- **D24 — Time-proportional horizontal spacing** (x ∝ onset) within a measure; **greedy
  line-breaking** into systems. (Optical spacing / Knuth–Plass deferred as overkill.)
- **D25 — Stems + beams (rhythm rendering) in the MVP.** Beam grouping by beat, slopes, flags
  for unbeamed notes. The fiddliest geometry in the system, but rhythm is core to banjo
  readability. Polish (rest styling, secondary beams) is follow-on.

## 10. Frontend architecture — resolved

Core loop: `edit → debounce ~150ms → core.compile(source, layout_config) → { render_tree,
diagnostics, highlight_tokens } → paint SVG + squiggles + highlighting` (latest-wins, drop
stale). Click note → span → editor selection; cursor move → span → highlight primitives (D20).

- **D26 — UI framework: Svelte.** Compiled, fast, least boilerplate; keeps the TS layer
  low-friction so effort stays on the Rust core. (Solid/React considered; not needed.)
- **D27 — Supporting frontend stack (all locked):**
  - **CodeMirror 6** as the editor (lightweight, built for embedding custom languages + diagnostics).
  - **Rust lexer is the single source** for syntax highlighting *and* diagnostics — it emits
    classified tokens + diagnostics with spans; CodeMirror renders them as decorations/lints.
    No second (JS/Lezer) grammar; zero highlight-vs-compile drift.
  - **Thin TS `core` adapter** hides Tauri-command vs WASM-export behind one `core.compile(...)`
    — the UI is written once, backend-agnostic (realizes D4).
  - **Minimal latest-wins async state:** editor owns source; one store holds latest
    `CompileResult`; small UI state (zoom/selection). No Redux-scale machinery.

## 11. Workspace, persistence, export & diagnostics — resolved

- **D28 — Workspace = core crate + thin wrappers.** `cadtab-core` (pure: whole pipeline,
  UI/IO-free, unit-testable) + `cadtab-wasm` (browser) + `src-tauri` (desktop) + Svelte `app/`.
  Shared types (model, render tree, diagnostics) live in `cadtab-core`; wrappers depend on it.
  Modules in core: `lexer`, `parser`, `ast`, `resolve`, `types`, `eval`, `model`, `instrument`,
  `stdlib`, `layout`, `render`, `diagnostics`.
- **D29 — Persistence = single-file `.ctab` docs.** One `.ctab` text file = one score. Lick
  libraries are separate `.ctab` files pulled in via `import`; the **stdlib lick set is
  embedded in the binary** (`include_str!`). Git-friendly, easy to share.
- **D30 — Export = SVG + PNG + PDF in MVP** (reuse render tree → SVG). SVG/PNG shipped in
  M5 (T5.3). **PDF is an MVP deliverable too** — it's the distribution standard for tab —
  but **sequenced post-M6** (tracked as T7.9; see 2026-06-27 changelog). The value of PDF
  is paginated, print-ready Letter/A4 output, which is a *layout-engine* feature (page
  breaks, systems-per-page, margins/headers), not a serialization add-on. It builds on
  M7's pinned-page layout (T4.7t) and shares the print styling (T5.3 / print-preview
  T7.6); sequencing it after M6's above-staff notation keeps pagination from being built
  twice.
- **D31 — Diagnostics = squiggles + hover, best-effort render.** CodeMirror squiggles +
  hover tooltips; the tab **renders its valid parts even with errors** (never blanks — honors
  resilient parsing, D19). Diagnostic = `{ severity, span, message, help: Option<…> }`.
  Dedicated problems panel deferred.

## 11b. Design review pass — refinements (D32–D39)

A final adversarial review surfaced notational gaps (where real tunes stress the design) and
two model tightenings. All folded into §5/§6 above.

- **D32 — Musical repeats + endings (voltas), in MVP.** A `repeat { … }` block renders a
  repeated section; nested `ending(1){} ending(2){} …` blocks are the voltas (body = the events
  before the first ending; `ending(k)` plays on pass k). **1st/2nd endings are in the MVP**
  (revised up from the earlier defer — too common in fiddle/bluegrass tunes to skip). To free the
  keyword, the programming unroll loop is **renamed `repeat N` → `loop N { }`** (amends D16).
  Lowered to flat measure attributes: `repeat_start`/`repeat_end` + `ending: Option<u8>`.
  (Replaces the earlier ABC-style `|: :|` marker idea — too notation-flavored for a programmatic
  language. D.S./coda still deferred.) Cost: adds volta-bracket layout + repeat-aware barring.
- **D33 — Explicit `pickup { }` block** for anacrusis: a partial bar excluded from the
  over/under-full barring diagnostic (D12), rendered offset.
- **D34 — Song metadata** (`title`, `composer`, `tempo`) as top-level declarations → rendered
  sheet header. `tempo` also seeds the future practice tool (Pillar B).
- **D35 — Instruments = builtins + `tuning` override.** Ship banjo & guitar; `tuning` overrides
  open-string pitches (double C, drop D, sawmill…). Full custom-instrument defs deferred.
- **D36 — Ties via `~` operator** (`3:2 ~ 3:2`) + a `tie` flag on the note for the renderer.
- **D37 — String numbering: 1-based, `1` = highest-pitched string** (banjo `5` = short drone).
  DSL literal `n` maps to `Vec<StringDef>` index `n-1`. Matches standard tab convention.
- **D38 — Web I/O = file-provider abstraction; multi-file on both targets.** `import` resolution
  in core goes through a **file-provider** (path → contents), not fs-coupled. Desktop (Tauri) backs
  it with the real filesystem (single `.ctab` + multi-file `import`); web (WASM) backs it with an
  in-memory map. **Web supports multi-file projects** via a single **project bundle** — a
  serialized `{ entry, files }` map (JSON for MVP; a zip-based `.ctabz` is a later option) that
  download/upload moves as one file and that populates the in-memory provider. The embedded stdlib
  stays available to the provider on both targets. *(Supersedes the earlier web =
  single-file/stdlib-only stance, so the M7 project dock + multi-file tabs are cross-platform, not
  desktop-only.)* The bundle is the browser-agnostic baseline (works on Firefox); File System
  Access API directory access stays an optional Chromium-only enhancement, not the dependency.
- **D39 — Model tightenings:** (1) chord/pinch = ONE shared `Duration` + `Vec<ChordNote>`
  (`{ pos, right_hand }`), not per-note durations (consistent with single-voice, D9);
  (2) rest literal `r` / `r_N` authoring `Event::Rest`.

Hygiene: merged the stale §4 checklist into §7b; defined `TimeSig`; simplified `CapoNote` to a
display label on `Score`; comment syntax = `//` line (and `/* */` block).

## 11c. Dependency stack — resolved (D40)

Per-area crate/library choices. Resolved forks: hand-rolled lexer · Svelte 5 + Vite SPA ·
`just` · browser-canvas PNG.

**`cadtab-core`** (pure; no UI/IO)
- `serde` + `serde_json` — serialization across the IPC/WASM boundary (D22)
- `thiserror` — typed library errors
- **hand-rolled lexer *and* parser** — no parser-gen, no `logos` (D18); full control over spans,
  error tokens, and highlight classification
- own `Duration { num, den }` rational (tiny; no `num-rational`)
- dev: `insta` (snapshots), `proptest` (property), `codespan-reporting` (pretty diagnostics in
  tests/CLI only — the app consumes the serialized `Diagnostic`)

**`cadtab-wasm`**
- `wasm-bindgen`, `serde-wasm-bindgen`, `console_error_panic_hook`; built with `wasm-pack`

**`src-tauri`**
- `tauri` 2; plugins `tauri-plugin-dialog`, `tauri-plugin-fs` (open/save + imports, D29/D38)
- `serde_json`, `anyhow` (wrapper glue)

**`app/`** — Svelte 5 (runes) + Vite SPA + TypeScript
- CodeMirror 6: `@codemirror/{state,view,language,lint,autocomplete,commands}`
- `@tauri-apps/api` + `@tauri-apps/plugin-{dialog,fs}`
- **SVG painted directly from the render tree — no d3/SVG lib** (thin painter, §3)
- **PNG export via browser canvas** (SVG → canvas → PNG; identical desktop + web, D30)
- state via Svelte runes/stores (no Redux, D27)
- dev: `prettier`, `eslint` (+ `eslint-plugin-svelte`, `typescript-eslint`), `svelte-check`,
  `vitest` + `@testing-library/svelte` + `jsdom`

**Tooling / CI**
- `just` — aggregate `just check` (fmt + lint + test, both languages) + dev tasks
- GitHub Actions: `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, node setup; runs `just check`
  + builds (incl. wasm + web)

- **D40 — Dependency stack** as above. Forks: hand-rolled lexer (consistency w/ D18 + learning);
  Svelte 5 + Vite SPA (least ceremony); `just` (language-agnostic runner); browser-canvas PNG
  (universal, zero-dep). Rejected: `logos`, SvelteKit, npm-scripts/cargo-make, Rust `resvg`.

## 11d. Workspace shell — view registry + editor groups (D41)

Post-freeze refinement: the UI shell grows tools beyond editor + render (print preview, a project
dock, a diagnostics/bottom bar, and later the Pillar-B mp3/mp4 looper). Captured so the shell rests
on a stable abstraction rather than ad-hoc panes.

- **D41 — Workspace = a view registry + an editor-groups layout (no free-floating docking).**
  - **Views are the unit of UI.** Every tool — editor, render, print preview, project dock,
    diagnostics, future looper/fretboard — is a registered *view* with a stable interface: `id`,
    `title`, `icon`, mount/unmount, and serializable state. The layout never hard-codes a tool; it
    places views. New tool = new registered view, no shell rewrite.
  - **Two view kinds.** **Global singletons** — one instance, not tied to a document (the left
    **project dock**, the **diagnostics / bottom bar**). **Document-bound** — bound to a specific
    `.ctab` (the **editor**, **render**, **print preview**, and later the **looper**). This lets
    "file A + its render" and "file B + its render" coexist in different groups; a render tab knows
    which file it renders.
  - **Layout = editor groups, not floating windows.** The area splits into **groups** (panes,
    side-by-side or stacked); each group holds a **stack of tabs** with one active. Supported: split
    (add a group), move a tab between groups, resize the splits, and **maximize ("zoom") a group**
    so it fills the area while the others stay open. The existing editor|render split is the N=2 /
    one-tab-each case of this.
  - **Explicitly deferred:** free-form drag-anywhere docking and floating windows — high cost, poor
    small-screen behavior, marginal payoff for a focused tool. Escape hatch if ever wanted:
    `dockview-core` / `golden-layout` (framework-agnostic, run under Svelte in WKWebView and the
    browser). Not adopted for MVP.
  - **Parity & dependencies.** Shell chrome (groups, tabs, dock, bottom bar) is pure JS → identical
    desktop and web. Multi-file projects work on every target (D38: live fs on desktop, project
    bundle on web); the only platform nuance is *how* the dock's tree is sourced — a live folder on
    desktop / Chromium-web (FSA API), or an uploaded/exported bundle on Firefox (no live folder).
    Document-bound multi-file views (tabs, project tree) presuppose M5 (open/save + `import`);
    the looper is Pillar B, landing later as a document-bound view. Builds on D26/D27 (Svelte,
    minimal latest-wins state): each view owns its small state; one store still holds the active
    document's `CompileResult`.

## 11e. Notation features — above-staff text & custom tunings (D42–)

M6 adds richer notation: section labels, chord symbols, bar numbers (all above-staff text), and
user-defined tunings. Decisions are captured here as each task lands.

- **D42 — Custom tunings extend D35's `tuning` directive, additively.** `tuning openG` (a named
  builtin) is unchanged; the directive now *also* accepts an inline per-string spec:
  `tuning { D4 B3 G3 D3 g4 }`, with an optional leading display-name string
  (`tuning "Open D" { … }`). Pitches use scientific notation — letter `A`–`G` (case-insensitive),
  optional `#`/`b` accidentals (repeatable), then a non-negative octave; `C4` = middle C = MIDI 60
  (`Pitch::from_name`). Strings are listed string 1 → n (matching D37 numbering and the header grid).
  The header tuning caption is now `Option<String>`: a named custom tuning shows its name, an
  **unnamed one shows no caption** (the circled-string grid still renders). The lexer treats `#` as
  an identifier-continuation byte so `F#4` lexes as one token (flats already lex via `b`); a bare
  `#` stays an error character. Validation mirrors the named path: a string-count mismatch or a
  malformed pitch diagnoses and the prior tuning stands. No new render-tree shape, so export and the
  live painter pick it up for free.

- **D43 — Section labels (rehearsal marks) + the above-staff band.** A score-body marker
  `section "A"` attaches a label to the *next* measure boundary: it falls on a barline (flushes the
  current auto-barred run, like a meter change) and stamps the label onto the measure that opens
  after it — threaded through eval as a `pending_section` parallel to `pending_meter`, landing on
  `Measure.section` (text + span, D20). Layout grows a reusable **above-staff band**: per system,
  vertical room is reserved above the staff (`band_top → staff_top`), stacked top→staff as *section
  label, then volta*. The label is a span-tagged `Text` of new role `SectionLabel`, left-anchored at
  the measure's start. This band is the shared machinery T6.2 (chord symbols, placed at a beat
  onset) and T6.3 (bar numbers) build on. The role flows to export for free via `tabStyle.ts` (D30).

- **D44 — Chord symbols (`chord "G"`) at a beat.** A beat-positioned annotation that places a
  chord name above the staff. Unlike section labels (measure-boundary, D43), chord symbols are
  *event-level*: the marker carries no duration and attaches its name to the **next event's onset**
  (threaded through eval as `pending_chord`, landing on `Event.chord`), so it works inside
  `measure`/`repeat`/`loop` blocks and bare runs alike; a trailing marker with no following event is
  dropped. `chord` is a **contextual keyword** — only an ident `chord` immediately followed by a
  string is the marker, so `chord` stays usable as an ordinary name (e.g. the stdlib's
  `forward_roll(chord)`); no reserved word added. Layout reuses the above-staff band (D43): chord
  symbols occupy a row below section labels and above voltas, each a span-tagged `Text` of new role
  `ChordSymbol`, centered over its note's column. Flows to export via `tabStyle.ts` (D30).

## 12. Phase 3 — MVP task order

Authored separately in **`docs/TASKS.md`** (per request) — a walking-skeleton-first,
dependency-ordered build plan.

## 13. Decision log

- *2026-06-20* — Phase 1 complete: D1–D4 locked (see §2). Phase 2 (architecture) started.
- *2026-06-20* — Musical model & instruments resolved: D5–D9 (see §5).
- *2026-06-20* — DSL core syntax resolved: D10–D13 (see §6).
- *2026-06-20* — Licks & functions resolved: D14–D17 (see §7). DSL design complete.
- *2026-06-20* — Compiler pipeline resolved: D18–D21 (see §8). Hand-rolled, resilient, fully spanned.
- *2026-06-20* — Layout & render-tree resolved: D22–D25 (see §9). Responsive reflow, stems+beams in MVP.
- *2026-06-20* — Frontend resolved: D26–D27 (see §10). Svelte + CodeMirror 6 + single-grammar highlighting.
- *2026-06-20* — Workspace/persistence/export/diagnostics resolved: D28–D31 (see §11). **Phase 2 complete.**
- *2026-06-20* — Design review pass: refinements D32–D39 (see §11b) + doc hygiene. **Design frozen for MVP.**
- *2026-06-20* — Revised D16/D32: musical `repeat { … ending(n){} }` with voltas **in MVP**; unroll loop renamed `repeat N`→`loop N`; dropped `|: :|` markers.
- *2026-06-20* — Dependency stack resolved: D40 (see §11c). Hand-rolled lexer, Svelte 5 + Vite SPA, `just`, browser-canvas PNG.
- *2026-06-27* — Workspace shell resolved: D41 (see §11d). View registry + editor-groups layout (splits / tab-stacks / maximize); document-bound vs global-singleton views; free-floating docking deferred. Reshapes M7.
- *2026-06-27* — Revised D38: `import` resolution via a file-provider abstraction; **web supports multi-file projects** via a single project bundle (JSON `{ entry, files }`), superseding web = single-file/stdlib-only. Shapes T5.1/T5.2 + keeps the M7 project UI cross-platform.
- *2026-06-27* — Custom tunings resolved: D42 (see §11e). Inline `tuning { … }` per-string spec (scientific-notation pitches, optional name) extends D35 additively; header caption now optional. Completes T6.4.
- *2026-06-27* — Section labels resolved: D43 (see §11e). `section "A"` marker → `Measure.section`; layout gains a reusable above-staff band (section over volta). Completes T6.1; T6.2/T6.3 reuse the band.
- *2026-06-27* — Chord symbols resolved: D44 (see §11e). `chord "G"` contextual-keyword marker → `Event.chord` (attaches to next onset); above-staff band gains a chord row (under section, over volta). Completes T6.2.
- *2026-06-27* — M5 (persistence & export) shipped: open/save `.ctab`, file-provider imports, project bundle, SVG/PNG export, new-from-template. **PDF confirmed an MVP deliverable, sequenced post-M6** (refined D30; tracked as T7.9): it's the distribution standard for tab, but it's paginated-layout work (not a serializer), so it lands after M6 settles above-staff layout and builds on M7's pinned page (T4.7t) — sequencing it later avoids building pagination twice, *not* dropping it from MVP.
