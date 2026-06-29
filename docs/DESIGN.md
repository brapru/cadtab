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
  but **sequenced post-M6** (tracked as T7.19; see 2026-06-27 changelog). The value of PDF
  is paginated, print-ready Letter/A4 output, which is a *layout-engine* feature (page
  breaks, systems-per-page, margins/headers), not a serialization add-on. It builds on
  M7's pinned-page layout (T7.17) and shares the print styling (T5.3 / print-preview
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

- **D45 — Bar numbering (`barnumbers lines|all|off`).** A top-level render directive (like `tempo`)
  setting a score-wide mode on `Score.bar_numbers` (`BarNumbers`, default `Lines`); an unknown mode
  diagnoses and keeps the default. Numbering is 1-based over the full bars — pickups (anacruses) are
  not numbered. Layout: `off` draws none; `lines` numbers the first numbered measure of each system;
  `all` numbers every measure. Numbers occupy the row **closest to the staff** (just above any
  volta), so the band stacks top → staff as section → chord → bar-number → volta; each a span-less
  `Text` of new role `BarNumber`, left-anchored at the measure start. The band's row baselines are
  computed once (`band_rows`) and each present row pushes the rows below it down, so absent rows
  cost no space.
  This completes M6's above-staff machinery; the role flows to export via `tabStyle.ts` (D30).

## 11f. M7 editor tooling & desktop polish (D46–D48)

Pre-MVP additions identified after M6: editor completions, a `.ctab` formatter, and a single
command source feeding both the in-app controls and the native desktop menu. Captured so the core
stays the single source of truth (consistent with D27's "Rust lexer is the single source").

*(Task IDs renumbered 2026-06-28 — see §11g and TASKS.md: D46→T7.24, D47→T7.25, D48→T7.20+T7.30.)*

- **D46 — Completions come from the core, not a second grammar (T7.24).** Autocomplete and inline
  hints are driven by the *existing* core knowledge: the keyword table (every keyword with a fixed
  value set — `instrument`, `tuning`, `barnumbers` — hints its options), the top-level operand
  shapes (`title` → a string), and the loaded stdlib/`def` registry (identifier completion). The
  `core` adapter exposes a completion query (position → candidates); CodeMirror renders them, tab
  accepts. No JS-side list of keywords/tunings to drift (mirrors D27). A setting toggles
  autocomplete + inline hinting off for users who want a quiet editor.
- **D47 — The formatter is a core pretty-printer over the parse tree (T7.25).** A pure
  `format(source) -> String` in `cadtab-core` that re-emits from the AST/token stream, so it is the
  one canonical layout of a document. Properties: **idempotent** (`fmt(fmt(x)) == fmt(x)`),
  **comment-preserving**, and **error-safe** — a document with parse errors is returned untouched
  rather than half-formatted (the resilient parser would otherwise drop the broken span). The UI is
  thin: a Format button and a format-on-save toggle both call the same core function. Lives in core
  (not the editor) so desktop, web, and any future CLI share one formatter.
- **D48 — One command source for in-app controls and the native desktop menu (T7.20, T7.30).**
  User actions (open, save, export-as, zoom in/out/reset, toggle dock…) are defined once as named
  commands; the toolbar/buttons and the Tauri native menu (View ▸ Zoom In, File ▸ Export…) both
  dispatch the same command, so the two never diverge. The **unified export control** (one button +
  SVG/PNG/PDF picker) is an instance: it replaces M5's separate buttons and routes every format
  through the existing io seam (binary write on desktop, download on web; D29/D30). The native menu
  is desktop-only (no-op on web); commands stay callable from the in-app UI on both targets.

## 11g. M7 re-scope from NOTES.md (D49–D51)

Mid-M7, a backlog of UX/workspace notes (`docs/NOTES.md`) was triaged into tasks and the remaining
M7 work was renumbered into one dependency-ordered sequence (T7.7–T7.34) so nothing is listed before
its blocker (the trigger was T7.9-PDF surfacing as "next" while blocked by the page-pin work). Three
items needed a decision:

- **D49 — Render is contextual; a lib renders a def-gallery (T7.16).** Render/preview is a property
  of what a file *is*: a file with a `score{}` renders its score; a **lib** (only `def`s, no score)
  renders a **gallery** that previews each `def` on its own page — answering "do imported files even
  have a render?". Needs **core** support to render a single `def` (e.g. synthesize a minimal score
  invoking it). Tab labels become the **filename**, with the view icon (editor/render/preview) doing
  the type distinction. Open sub-decision deferred to T7.16: how to render a **parameterized** `def`
  (representative/default args, nullary-only, or a placeholder).
- **D50 — Splits stay horizontal for MVP (T7.12).** The workspace remains a single horizontal row of
  groups; the split control offers **left/right** only. Full 2D nested splitting (up/down → a split
  tree) is deferred — it's a substantial rewrite of the D41 layout and isn't needed to ship.
- **D51 — Icons: self-hosted Material Symbols (T7.10).** The desktop app must work fully offline, so
  icons are **bundled at build** (font/SVGs in the app), never CDN-loaded (which would rule out a
  remote Google Fonts load). Material Symbols, self-hosted. A small icon wrapper gives one usage
  convention; the topbar/controls move from text to icons + styled tooltips (T7.14).

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
- *2026-06-27* — Bar numbering resolved: D45 (see §11e). `barnumbers lines|all|off` directive → `Score.bar_numbers` (default `lines`); band gains a top bar-number row. Completes T6.3 — **M6 done** (D42–D45).
- *2026-06-27* — Pre-MVP M7 additions scoped: D46–D48 (see §11f). Core-driven completions (D46), core `.ctab` formatter (D47), one command source for in-app + native desktop menu incl. unified export (D48). Adds T7.10–T7.14, T8.6 to the plan; core stays the single source of truth (cf. D27).
- *2026-06-27* — M5 (persistence & export) shipped: open/save `.ctab`, file-provider imports, project bundle, SVG/PNG export, new-from-template. **PDF confirmed an MVP deliverable, sequenced post-M6** (refined D30; tracked as T7.9): it's the distribution standard for tab, but it's paginated-layout work (not a serializer), so it lands after M6 settles above-staff layout and builds on M7's pinned page (T4.7t) — sequencing it later avoids building pagination twice, *not* dropping it from MVP.
- *2026-06-27* — M7 started; T7.1 shell foundation landed (D41, §11d) at **incremental scope**: pure `workspace.ts` model (view registry; groups → tabs → active; weights) + `Workspace.svelte` chrome (tab strips, resize gutters, per-group maximize) mounting views via a snippet; `App.svelte` drives the editor|render split through it. Adding groups (split), moving tabs between groups, and layout serialization deferred until a second tab exists (T7.4/T7.5). Views T7.2–T7.6 register on top.
- *2026-06-27* — T7.3 bottom status bar landed (D41 global singleton): `BottomBar.svelte` as fixed chrome below the workspace, hosting a dock toggle (Cmd/Ctrl-B → `dockOpen`; panel is T7.2) and a live problem indicator (error/warning counts via pure `diagnosticCounts`). Global singletons are registered in the view registry but mounted as chrome, not tabbed. Added shared `--error`/`--warning` theme tokens. Diagnostics panel + jump-to-span deferred to T4.7m.
- *2026-06-27* — T7.2 left project dock landed (D41 global singleton): `Dock.svelte` mounted left of the workspace, toggled by the T7.3 `dockOpen` seam; lists entry + bundle libs via pure `project.ts`. Display-only — opening a file as a tab is T7.4. Confirmed the **live-folder (FSA) tree source from D38 is not yet implemented** (open is single-file or one bundle), so the dock lists the loaded bundle map; folder-tree rendering + live-folder watching are deferred refinements.
- *2026-06-27* — T7.5 render-as-document-bound-view landed: turned on the deferred D41 move/split verbs with the render as first consumer. `workspace.ts` gains `moveTab` (tab drag between groups; emptied groups drop) and `splitTab` (pop active tab into a new group); `Workspace.svelte` makes tabs draggable, groups drop targets, plus a keyboard-reachable Split button. The render is now placeable in any group. Multi-document render coexistence rides this once T7.4 adds a second doc.
- *2026-06-27* — T7.4 decomposed into a model refactor then the UX. **T7.4a landed:** App's single-document state moved into a keyed session store (`documents.ts` — `DocStore`/`DocSession` + pure ops); the active doc's `source`/`name`/`path`/`dirty` derive from it. Behavior-preserving (one session this phase), green against the existing App suite. T7.4b will give each opened/imported file its own id + editor tab, wire the dock to open files, and make compile/selection per-doc so two renders coexist.
- *2026-06-28* — **T7.6 print-preview landed — Phase A (shell skeleton) complete.** `PreviewView.svelte` (registered `preview` document-bound view) shows a doc's print output by running its live render tree through the export serializer (`renderTreeToSvg`) inline — light, self-contained, theme-independent; no second layout pipeline. Opened from a topbar **Preview** button as a tab beside the render. Paginated print/PDF is still T7.19. Phase A done: foundation (T7.1) + bottom bar (T7.3) + dock (T7.2) + render view (T7.5) + multi-file tabs (T7.4) + preview (T7.6). *(Subsequent plan re-scope superseded the "next" pointer — see the renumber entry.)*
- *2026-06-28* — **M7 re-scope + task renumber (D49–D51, see §11g).** Triaged a `docs/NOTES.md` backlog (18 UX/workspace items) into tasks and renumbered all remaining M7 work into one dependency-ordered sequence **T7.7–T7.34** (TASKS.md) so nothing is listed before its blocker — the trigger was T7.9-PDF showing as "next" while blocked by the page-pin work (now T7.17). New: 3 bug fixes (group sizing, project-clear-on-open, page-scroll), an icon foundation (Material Symbols, self-hosted, D51) + iconified chrome, tab-strip group controls (+/maximize/close/Fit/split — horizontal only, D50), close-tab, drag-cue refinement, open-as-folder, contextual def-gallery render (D49), code-folding, and a help view. Old→new map in TASKS.md (e.g. T4.7t→T7.17, old-T7.9→T7.19, T4.7n→T7.34). Completed T7.1–T7.6 keep their IDs.
- *2026-06-28* — **Desktop (WKWebView) parity fix for T7.5/T7.4b.** Tab dragging used HTML5 drag-and-drop, which is intercepted/unreliable in WKWebView (Tauri's desktop webview) though it worked on Chromium/web; reimplemented on **pointer events** (the same approach as the gutter, which already worked on desktop) — press/threshold/hit-test-by-rect/`moveTab`, with guarded `setPointerCapture`. Active-follows-focus relied on CodeMirror's DOM `focus` event; added a `pointerdown` path on the editor pane + render so focusing a doc is reliable on WKWebView too. No Tauri config change needed (pointer drag coexists with the default OS drag handler). Reinforces the WKWebView-divergence rule: prefer pointer events over HTML5 DnD / sole reliance on DOM focus for desktop.
- *2026-06-28* — **T7.16 landed — contextual render (def-gallery) + filename tab labels (D49).** Render is a property of what a file *is*: a file with a `score{}` renders it; a **library** (top-level `def`s, no `score`) renders a **def-gallery** — a preview card per declared def. `compile` branches `!has_score && has_def` → `eval_def_gallery` + `layout_gallery`, so `CompileResult` is unchanged and every consumer (render/preview/export) gets it free, with no wasm/TS binding churn. Resolved the open sub-decision: a **parameterized** def previews under **representative sample args** — every parameter bound to one canonical sample chord (the open melody strings 3-2-1, `c.0` lowest, matching the roll convention and the `g_chord` example) at default 1/8 — best-effort, with a def that errors or renders nothing falling back to a **signature-only card**; the synthetic invocation's diagnostics are discarded (the library's real diagnostics still come from the resolve/type passes). **Provisional** — the eventual direction is an author-specified example invocation per def (cf. the stdlib's own "provisional" licks). Two new render roles (`DefHeading`/`DefNote`) added to the core enum and both painters (`tabStyle.ts` + the `Tab.svelte` live CSS). Separately, **tab labels became the filename** (the icon now carries the editor/render/preview distinction; the missing-on-disk strike rides the filename), and tab close is a uniform "Close tab" wired to **Cmd/Ctrl-W** (closes the focused tab; `preventDefault` keeps the desktop webview's window). `examples/licks.ctab` added as an openable library. **First task in the T7.7–T7.34 sequence to land; reinforces the core-as-single-source rule (D27/D46).**
- *2026-06-27* — **T7.4b landed — multi-file editing.** Each opened/imported file gets its own `docId`, editor tab, render, and latest-wins compiler; compile output/highlight/layout-width are per-doc maps. Open/New/dock **add or focus a tab** rather than replacing (discard-on-open guard removed — opening never loses work). Active-follows-focus (Editor `onFocus` + Workspace `onActivateView`) drives the topbar/Save/Export; the dock opens files on click; editing a lib syncs `projectFiles` and recompiles dependents. New `RenderView.svelte` owns each render's pane width/reflow; the snippet keys views by instance so a doc switch mounts a fresh editor. **Deferred:** closing tabs; keep-alive across *stacked*-tab switches (a switch remounts — undo/scroll reset; side-by-side groups keep both mounted); multi-project import isolation (`projectFiles` is the current project context); per-doc zoom. **T7.4 done; Phase A (shell skeleton) complete.**
- *2026-06-28* — **T7.7 fix: group sizing after move→split→move.** Raw group weights can sum to under 1 after move/split churn (or when a sub-1-weight group is maximized alone), and a `flex-grow` total below 1 leaves the row partly empty — cutting the view off. `Workspace.svelte` now normalizes each group's `flex-grow` over the *visible* groups (`weight / totalWeight`), so it always sums to 1 and the row fills while ratios are preserved — the model's raw weights (`moveTab`/`splitTab`) stay untouched.
- *2026-06-28* — **T7.8 fix: opening a project replaces the prior one.** Opening a new project used to leave the old project's docs/tabs/renders open (a stale render lingered) — a gap from T7.4b's "open adds a tab" rule, which is right *within* a project but wrong *across* projects. `App.svelte`'s `openDoc` now splits the two: the `context` branch (a single score or bundle from disk = a project open) resets the doc store to the new entry, rebuilds a fresh `defaultWorkspace`, clears the per-doc maps + live-compiler/edit-handler caches (`resetDocState`), and resets `projectFiles`/`bundlePath`/`projectEntryName`; dock-opened libs and New-from-template omit `context` and still add tabs. Since replacing can discard unsaved work, `openFile` guards with a **dirty-only confirm** before the file picker (clean projects swap silently; declining aborts before any dialog). The confirm is a **custom in-app modal** — a presentational `ConfirmDialog.svelte` (themed with the app tokens; backdrop is a real labelled `<button>` for accessibility, kept out of the tab cycle; Escape cancels, Enter confirms, the confirm button auto-focuses, and **Tab is trapped** within the dialog's controls so focus can't wander to the chrome behind it) driven by an `askConfirm({message, …}) → Promise<boolean>` controller in `App.svelte` that holds the open prompt + its resolver. This replaced a first attempt on the native `window.confirm`, which **silently no-ops in WKWebView/WRY** (Tauri's webview) so no prompt showed on desktop — and is more cohesive with the UI than the system/browser dialog regardless. (Reinforces the WKWebView-divergence rule: native JS dialogs aren't reliable in the desktop webview — use our own DOM or the Tauri dialog plugin.) The modal is reusable for future confirms; folder open (T7.15) will reuse this project-replace path.
- *2026-06-28* — **T7.9 fix: only panes scroll, not the page.** A tall render scrolled the whole shell instead of the render pane. Cause: `RenderView`'s `.render-pane` was `flex: 1` in a column flex container without `min-height: 0`, so its min-height defaulted to content height — the pane grew to fit the render (pushing the shell) rather than shrinking and engaging its own `overflow: auto`. Fix: `min-height: 0` on `.render-pane`. Also hardened the chrome against any future leak: `main` gets `overflow: hidden` and `html, body` get `overflow: hidden` (app.css), so the shell is pinned to the viewport and scrolling only ever happens inside the view bodies (editor/render/preview/dock), each already constrained. (General rule: every `flex: 1` pane that scrolls needs `min-height: 0` — and `min-width: 0` — or its `overflow` won't engage.)
- *2026-06-28* — **T7.10 landed — self-hosted Material Symbols icon foundation (D51).** The desktop app must render icons fully offline, so the Material Symbols set is bundled at build, never CDN-loaded. Chose the **variable woff2 + ligatures** over curated inline SVGs (decision presented to the user): added the `material-symbols` dep and committed the full outlined variable font to `app/public/fonts/material-symbols-outlined.woff2` (~3.9 MB — the size cost of skipping curation, so *any* symbol name "just works"), `@font-face`'d locally in `app.css` and served at `/fonts/…` (relative, no network). New `Icon.svelte` is the single icon-usage convention the chrome draws from: `name` (the ligature) + `size` (px number or CSS string) / `fill` / `weight` variable-axis props / optional `label`; **decorative by default** (`aria-hidden`, no role, `translate="no"` so the browser can't mangle the ligature text), upgrading to `role="img"` + `aria-label` when labelled. `-webkit-font-feature-settings: "liga"` is set alongside the standard property so WKWebView (Tauri's desktop webview) renders the name→glyph ligature (WKWebView-divergence rule). This is foundation only — T7.12 (tab-strip group controls) and T7.14 (iconified topbar + styled tooltips) swap the current text-glyph (`◫ ▢ ▣`) / emoji chrome over to it. Tests in `Icon.test.ts` (ligature/a11y/size/axes).
- *2026-06-28* — **T7.12 landed — group controls in the tab strip (built in confirmed chunks).** The per-group control set moved into the tab strip as T7.10 icons and gained New/Fit, replacing scattered topbar/render-toolbar controls. (1) **Render launcher:** a contextual ♪ (`music_note`) control in the active group's control set, shown when its active tab is an editor (mirroring how Fit shows for a render) — spawns-or-focuses that doc's render (idempotent `addTab`), filled-accent with a "Go to render" label when already open, closing the T7.11 gap where a closed render had no way back; `openPreview`/`openRender` share one `openViewFor(docId, type)`. *(First shipped on each editor tab beside the close button, then moved into the control group on review.)* (2) **New "+":** opens a template popover menu (dismiss on outside-pointerdown / Escape); the topbar New `<select>` was removed; an **empty-tabs placeholder** keeps New reachable once every tab is closed, and `openDoc` reseeds a fresh `defaultWorkspace` from empty. (3) **Fit** moved into the group controls (`crop_free`) and the in-pane render zoom toolbar (`− % + Fit`) was deleted — zoom now lives only on Cmd/Ctrl +/− (RenderView no longer takes zoom callbacks). (4) **Double-click a tab** toggles its group's maximize. (5) **Iconified** the tab type-icons (the `VIEWS` registry `icon` field is now a Material Symbols ligature name — `code` / `music_note` / `preview`) and the split (`split_scene`) / maximize (`open_in_full` ↔ `close_fullscreen`) controls. (6) **Active-group-only:** the control set (New/Fit/split/maximize) renders only on the active group; the per-tab close + render-launcher stay on every tab. **Decision:** "active group" is tracked as the **last group a pointer went down in** (local `controlGroupId` in Workspace, default first group; a maximized group owns the controls) rather than derived from the active document — because the default editor|render layout places one doc's two views in two groups, so the doc id alone can't distinguish them and Fit (render-group-only) would never appear. Tests across `workspace.test.ts` and `App.test.ts`. **Spun off T7.15b** (raised mid-task): New should create an unsaved *dirty draft listed in the dock*, deferred to ride with the dock-as-folder rework (T7.15).
- *2026-06-28* — **T7.14 landed — iconified topbar + styled tooltips.** Two parts. **(1) Tooltips:** a reusable `use:tooltip={text}` Svelte action (`lib/tooltip.ts`) renders a CSS-styled chip (`.app-tooltip` in app.css) **portaled to `<body>`** and positioned by JS on hover/focus, dismissed on leave/blur/activation. Chosen over a pure-CSS `[data-tip]::after` because pseudo-element tooltips are clipped by `overflow:hidden/auto` ancestors (the tab strip, the dock list) — the portal guarantees full coverage. Every native `title=` across the app (topbar, Workspace controls, BottomBar, Dock) was replaced with it. **(2) Iconified topbar:** the remaining text buttons became square icon buttons (`.icon-btn`, labelled by `aria-label` + tooltip) — Open `folder_open`, Save `save`, Save Project `save_as`, Preview `preview`, Theme `brightness_auto`/`light_mode`/`dark_mode` (`theme.ts`: `themeGlyph`→`themeIcon`). The two Export buttons **collapsed into one `download` menu** (an SVG/PNG popover mirroring the New "+" menu; dismiss on outside-pointer/Escape) — chosen over two distinct export icons. Tests: `tooltip.test.ts`, updated `theme.test.ts`, and `App.test.ts` (button queries moved to `aria-label`; exports go through the menu; Export menu open/dismiss covered).
- *2026-06-28* — **T7.13 landed — drag cue highlights the open drop space in the tab bar.** The tab-drag target cue moved off the whole `.group` section (a 2px accent outline that boxed the entire group) onto a dedicated **`.dropzone`** — the open strip space after the tabs where a dropped tab lands (a `flex: 1` filler that also pushes the group controls to the right edge). `.dropzone.droptarget` gets a translucent accent wash + an `inset` accent bottom edge, so only that empty space is cued — not the existing tabs, the view body, or the whole group. Refines the T7.5 drop cue; test in `workspace.test.ts`. *(Iterated on review: task wording said the group body; first tried a body overlay, then the whole tab strip, settling on just the open drop space.)*
- *2026-06-28* — **T7.12 follow-up — zoom is per focused view type.** Cmd/Ctrl +/− previously scaled only the render; now it targets whatever view the user is focused on. Split the single `zoom` into `editorZoom` + `renderZoom` (App), routed by a `focusedKind` set from the same activate/focus paths as active-follows-focus (`focusView`); the editor applies its zoom as an inherited `font-size` (`{zoom}em`) on the CodeMirror container, so focusing an editor and pressing Cmd/Ctrl + grows the **code text**. Cmd/Ctrl 0 resets the focused type; the tab-strip Fit control stays render-only (`fitRender`). Scope decision: **per view type** (one editor-zoom shared by all editors, one render-zoom shared by all renders) over per-tab/per-doc — simplest, matches typical editor font-zoom, and sidesteps the per-doc-zoom state deferred in T7.4b. Tests updated in `App.test.ts` (focus render vs editor, then assert the right target scales); `Editor.svelte` gained a `zoom` prop.
- *2026-06-28* — **T7.11 landed — close tab.** Each tab gained a close affordance (the T7.10 `Icon` `close`), added as a **sibling** button of the draggable tab button (wrapped in `.tab-wrap`) so the pointer-drag/activate handlers stay untouched and no interactive element nests inside another. New pure model ops in `workspace.ts`: `closeTab` (remove one view, drop the group it empties like `moveTab`, keep the group's active tab unless it was the closed one, un-maximize a vanished group — can leave an empty layout) and `docIdsWithViews`; plus `removeDoc` in `documents.ts` (drop a session, fall focus to the last remaining or null). **Close semantics (user decision):** every view closes **independently** — closing a tab removes just that view instance — and a document's session **outlives its individual views**, cleaned up only when its *last* view closes (so a render can outlive its editor). Two unsaved-changes guards via the in-app `ConfirmDialog` (the native `confirm` no-ops in WKWebView, hence our own modal): closing the **editor of a dirty doc** warns ("its unsaved changes stay in the document's other open views"), and closing a dirty doc's **last view** warns the changes "will be lost." `App.closeView` orchestrates the guard + `closeTab`; once a doc has no remaining views, `cleanupDoc` drops that doc's per-doc maps and live-compiler/edit-handler caches, `removeDoc` drops the session, and `reconcileActive` re-points the active document at whatever view survives. Closing the final tab empties the workspace; the New/Open path reseeds a fresh `defaultWorkspace` so the app recovers from the empty state. (An earlier draft coupled editor-close to whole-document-close; revised to independent views per the user.) Tests added across `workspace.test.ts`, `documents.test.ts`, and `App.test.ts`.
- *2026-06-28* — Bug fix: an unterminated block (e.g. `score {` with no `}`) reported the error but drew no editor squiggle. The diagnostic existed (counted in the bottom bar) but its span was the **zero-width EOF point** past the source, which `placeDiagnostics`/`spanToRange` drop (nothing to underline). Two-part fix: (1) the parser's four braced-block sites (`parse_score`/`parse_block`/`parse_repeat`/`parse_tuning_strings`) now close via a new `expect_rbrace(open)` that, at EOF, anchors the diagnostic on the **unclosed opening `{`** (a real, underline-able span) with a clear "this `{` is never closed; expected `}`" message + help — better UX than underlining the file end; (2) a general frontend safety net in `placeDiagnostics`: a zero-width diagnostic span now falls back to the character just before the point, so *any* counted problem (the sibling `]`/`)` EOF cases too) stays visible as a squiggle. Out-of-range (stale-compile) spans are still dropped.
- *2026-06-28* — **T7.15 Chunk A landed — hierarchical dock tree (cross-platform).** First sub-chunk of open-as-folder: the dock graduates from a flat libs list to a real **folder tree**, drawing on whatever path structure the project already has (a bundle's nested paths today; a live folder in Chunk B). Pure `projectTree(files)` in `project.ts` folds each `/`- or `\`-split path into nested folder nodes (every segment but the last is a folder; the last is the file leaf), **folders before files, each alphabetical** (case-insensitive), with folders keyed by their **full path prefix** so expand/collapse state survives tree rebuilds. `Dock.svelte` renders it with a **recursive Svelte snippet** (depth → CSS indent via `--depth`), per-folder expand/collapse held in an immutable `Record<string,true>` of *collapsed* paths (default expanded; matches the repo's reassign-immutable idiom over a reactive `Set`, which the `svelte/prefer-svelte-reactivity` lint forbade). **Tree iconography (user pick):** the **folder/folder_open glyph itself toggles** (no separate chevron) and files carry `music_note` — both via `Icon.svelte`. Dock props/`onOpenFile` contract unchanged, so App needs no change this chunk; the privileged-"entry" removal + live-folder source come in Chunk B. Tests in `project.test.ts` (fold/sort/nesting/prefix-keys/entry-flag) and `Dock.test.ts` (folder rendering, click-to-collapse). **Sub-chunk plan + desktop-watches/web-snapshots line recorded in TASKS.md T7.15.**
- *2026-06-28* — **T7.15 Chunk B1 landed — IDE dock model (no privileged entry) + drafts-in-dock (folds in T7.15b).** Reworked the project/document model so the dock is a real IDE file tree. **(1) No entry:** doc ids are now scheme-prefixed — `file:<key>` for a project file (the key is its dock/import path) and `draft:<n>` for an unsaved draft — and `projectFiles` holds **every** file in the project (the shared import map passed to every compile), not just the libs-minus-entry. The old `lib:`/`entry:`/path id scheme and the privileged "entry" doc are gone. **(2) `project.ts`** moved from `projectFileList`/`ProjectFile` to a `DockEntry`-based tree: `fileEntries(files)` builds path-keyed rows (dirty per row) and `projectTree(entries)` renders them, with **path-null entries (drafts) as root leaves**. `Dock.svelte` takes `entries`/`activeKey`/`onOpen` and shows a **dirty dot** on any unsaved row. **(3) T7.15b half-a (never-saved-dirty):** `documents.ts` `DocSession` gained `everSaved`; `isDirty` is `!everSaved || content !== savedContent`, so a **New draft is dirty from birth** and clears on first save (`markActiveSaved` sets `everSaved`). **(4) T7.15b half-b (drafts in dock):** App's derived `dockEntries` lists every `projectFiles` key (dirty iff its open doc is dirty) **plus every open `draft:` doc** as a root leaf. **Decisions (user):** drafts render *in-tree* with the same `music_note` + a dirty dot (not a separate "Unsaved" section); the built-in **starter stays clean** as a deliberate special-case (`everSaved` default true) so the app doesn't open looking unsaved, while user-created New drafts are dirty. **(5)** App's `openDoc` split into `openProjectInto` (replace the project — reset map/paths/name, drop old docs/tabs, open one file), `addOrFocusFile` (dock click — add/focus a tab), `newDraft` (New), and `onOpenEntry` (route a dock click: file vs draft); a lone opened score is now a **one-file project** (keyed by fs path on desktop, name on web) so it lists in the dock too. `filePaths` (key → fs path) is threaded now for Chunk B2 write-back/import-base. "Save Project" still names one bundle `entry` at the serialization boundary (the active doc's key, or a derived name for a draft) since the `.ctabz` format requires it. Tests updated/added across `documents.test.ts`, `project.test.ts`, `Dock.test.ts`, `App.test.ts` (drafts-in-dock + clean-starter/dirty-New). **Chunk B2 next:** desktop Tauri open-folder + write-back to the live folder.
- *2026-06-28* — **T7.15 Chunk B2 landed — desktop folder open + write-back, and an open/save reframe (D52).** **(1) Folder IO:** `io.ts` gained pure, injectable folder reading — `joinPath`/`toRelative` (handle `/` and `\`; keys normalized to forward slashes), `collectCtabFiles(root, readDir, readFile)` (recurses the tree collecting `.ctab` files, skipping dot-dirs; returns key→contents + key→abs-path), and thin Tauri `openFolder()` glue (directory picker → `collectCtabFiles`). The fs access is injected so the recursion is unit-tested with fakes (the plugin calls are the only glue). **(2) Folder open:** `openProjectInto` gained an `openKey: null` mode — a folder populates the dock tree but opens **no file** (the workspace rests on its empty-tabs placeholder, VS-Code-style; **user pick**). Opened files carry their real fs `path` (from the threaded `filePaths`), so the existing `saveFile` (overwrite-in-place on desktop) **writes straight back to the live folder** — no new save path needed. Desktop-only; `openFolder` resolves null on web (FSA directory access is the deferred Chunk D). **(3) Open reframe (D52, user-driven):** the topbar **Open button is now web-only**; on desktop you open a file/bundle via **Cmd/Ctrl+O** and a folder via **Cmd/Ctrl+Shift+O** + a **dock-header Open Folder** control — the rationale being that a single file and a folder both resolve to "a project" internally (since B1), so the only real difference is the OS dialog mode (file vs directory picker), and that file-vs-workspace split is conventional (cf. VS Code). The native **File ▸ Open** menu that completes desktop discoverability stays scoped to **T7.30**. **(4) Save reframe (D52, user-driven):** **"Save Project" is gone** — the awkward save-file/save-project duality didn't fit the live-folder model (the folder *is* the project; you save files into it). The `.ctabz` bundle moved to an **Export ▸ Bundle (.ctabz)** menu item (a derived artifact like SVG/PNG: always prompts, never rebaselines the doc's saved state), leaving exactly one **Save** (Cmd/Ctrl+S → the current file's real path, or a prompt for a never-saved draft). Consistent with D38 (the bundle is the portable/shareable snapshot, not the primary store). Tests: `io.test.ts` (path helpers, `collectCtabFiles` nesting/dot-skip, mocked-Tauri `openFolder` + cancel + web-null), `Dock.test.ts` (Open Folder control fires / absent without the callback), `App.test.ts` (desktop mode via a `__TAURI_INTERNALS__` toggle + mocked `invoke`: Open hidden, dock folder-open into the tree with no file open, save writes back to the real path, bundle-via-Export prompts). **Chunk C next:** desktop fs-watching (tree + clean-file sync; an external change to an open *dirty* buffer surfaces a notice, never a silent clobber). Chunk D (web FSA + Refresh) optional.
- *2026-06-28* — **T7.15 Chunk C landed — desktop live-folder watching (auto-reload).** An open folder now stays in sync with disk. **(1) IO (`io.ts`):** `rescanFolder(root)` re-reads the `.ctab` tree (reuses `collectCtabFiles`); `watchFolder(root, onChange)` wraps Tauri `watch` (recursive, debounced `delayMs: 200`) and returns the unwatch fn — a no-op off-desktop. Re-scanning the whole folder on *any* event (rather than decoding the platform-specific `WatchEvent` payload) keeps the reconcile robust. **(2) Pure reconcile (`watch.ts`):** `reconcileScan(scan, openContent)` adopts the fresh scan as the project map outright (added files appear in the dock, deleted drop) and queues a reload for each open file whose disk content diverged from its buffer. **(3) App wiring:** reintroduced `projectRoot` (dropped in B2), watched via an `$effect` that tears down on project-swap/unmount; on an event it re-scans → reconciles → updates `projectFiles`/`filePaths`, reloads diverged tabs, and recompiles open files. Reloads reach the **live CodeMirror** through the Editor's pre-existing-but-unwired `loadRequest={{content, token}}` prop — bumping the token swaps the editor state to the disk content, resetting undo, and (since `view.setState` dispatches no transaction) without echoing back through `onChange`. `documents.ts` gained `reloadDoc` (replace buffer + rebaseline clean). **Decision (user-driven): always auto-reload, NO notice UI** — the earlier plan (protect a dirty buffer / surface a conflict banner/modal) was overridden in favour of "just auto-update the tab": disk is the source of truth and an external change reloads straight in **even over unsaved edits** (last-write-wins; a user's own Save still overwrites disk). This dropped the entire conflict-notice question. Watching is **desktop-only** (Tauri notify); web folders would need polling (no FSA change API), so they stay snapshot + manual Refresh in the optional Chunk D. Tests: `watch.test.ts` (adopt-scan, reload-on-diverge incl. over-dirty, no-reload-when-matching), `documents.test.ts` (`reloadDoc`), `io.test.ts` (`rescanFolder`, `watchFolder` recursive-wire + off-desktop no-op), `App.test.ts` (desktop: an open file live-reloads to disk content when a synthetic watch event fires). **T7.15 desktop live folder is complete (Chunks A–C); only the optional web-FSA Chunk D remains.**
- *2026-06-28* — **T7.15 Chunk C fix — folder open + watch were silently denied by missing fs capabilities.** Live-folder watching (and the folder scan itself) did nothing on the real desktop app: `src-tauri/capabilities/default.json` granted `fs:allow-read-text-file`/`write-text-file`/`write-file` but **not** `fs:allow-read-dir`, `fs:allow-watch`, or `fs:allow-unwatch` (Tauri's `fs:default` set doesn't include them). So `readDir` (used by `openFolder`/`rescanFolder`) and `watch` were **denied at the IPC layer** — and because the `$effect` set up the watch with a bare `.then()` (no `.catch`) and `onFolderChanged`/`openFolderFlow` swallowed errors (the latter via `window.alert`, itself a no-op in WKWebView), the denial was invisible. The Vitest suite passed throughout because it mocks `@tauri-apps/plugin-fs`, so the capability gap never surfaced there. Fix: added `fs:allow-read-dir`, `fs:allow-watch`, `fs:allow-unwatch` to the default capability (the `$HOME/**` scope already covered the paths); and made the failures **observable** — the watch-setup promise now `.catch`es with a `console.error`, and `onFolderChanged`/`openFolderFlow` `console.error` on failure (alongside the web alert) so a future capability/scope gap is debuggable rather than silent. Capability changes are baked into the Rust binary, so they require a `just dev` restart (not just frontend HMR). Reinforces the WKWebView-divergence rule (native `alert` is a no-op there — log, don't rely on it) and adds a corollary: desktop fs features need their **explicit `fs:allow-*` capability**, not just `fs:default`.
- *2026-06-28* — **T7.15 Chunk C fix (part 2) — the watcher needed the fs plugin's `watch` Cargo feature.** After the capability fix, opening/re-scanning a folder worked (the `fs:allow-read-dir` grant) but live watching still never fired. Root cause: `tauri-plugin-fs` gates its notify-based watcher behind a Cargo **`watch` feature** (`watch = ["notify", "notify-debouncer-full", …]`; the `watch`/`unwatch` commands are `#[cfg(feature = "watch")]`-compiled). Our `src-tauri/Cargo.toml` pulled `tauri-plugin-fs = "2.5.1"` with **no features**, so those commands weren't in the binary and the JS `watch()` call failed (now visibly, via the Chunk-C `console.error` catch). Fix: `tauri-plugin-fs = { version = "2.5.1", features = ["watch"] }` — `cargo check` confirms `notify`/`notify-debouncer-full` now compile in. So live-folder watching needs **three** things, all desktop-only and none caught by the fs-mocking Vitest suite: the JS call, the `fs:allow-watch`/`fs:allow-unwatch` **capability**, and the plugin's **`watch` Cargo feature**. Requires a `just dev` rebuild (Rust binary).
- *2026-06-28* — **T7.15 Chunk C fix (part 3) — deletes weren't watched; switched `watch` → `watchImmediate`.** With watching finally live, file *creates* synced to the dock but *deletes* didn't (confirmed via a temp `console.debug`: the re-scan callback fired on `touch` but never on `rm`). Cause: Tauri's debounced `watch` uses `notify-debouncer-full`, whose file-identity tracking **drops removal events on macOS/FSEvents**. Fix: `watchFolder` now uses **`watchImmediate`** (raw `notify`, forwards every event including `Remove`); since that fires per-raw-event (a single save can emit several), the App `$effect` wraps the re-scan in the existing `debounce` (150 ms) to coalesce. Both JS fns invoke the same `plugin:fs|watch` command, so the `fs:allow-watch` capability already covers it — no Rust/capability change. Also hardened `collectCtabFiles` to **skip a listed file that errors on read** (a delete racing a re-scan) rather than aborting the whole scan. The temp diagnostic was removed. Tests updated (`watchFolder` asserts `watchImmediate`; `collectCtabFiles` skips an unreadable file). **Live add + delete now both sync.** Frontend-only change — Vite HMR picks it up, no `just dev` restart needed.
- *2026-06-28* — **T7.15 Chunk C enhancement — "missing on disk" tabs (delete/move) with save-back.** A live-folder file deleted or **moved** out from under an open tab (a move = delete-at-old + create-at-new) now shows that tab's label **struck-through + dimmed**, keeping the buffer fully editable; a **Save rewrites the file to its original path** (already works via `saveDocument`'s create-or-overwrite) and clears the strike. New `DocSession.missingOnDisk` (default false) + `markMissingOnDisk(store, isMissing)` set it in `applyScan` from the fresh scan's key set (present again clears it; unchanged docs keep their object reference to avoid reactive churn); `markActiveSaved`/`reloadDoc` clear it. `App.handleEdit` now syncs a `file:` edit into `projectFiles`/the dock **only while its key is in the project**, so editing a missing doc doesn't resurrect a phantom dock row before it's saved back. `Workspace` gained a `missingDocIds` prop and strikes matching tab titles (`.tab-title.missing`). Note: tab labels are still the **view type** ("Editor"/"Render") until T7.16 puts the filename there — the strike applies to whatever the tab shows and upgrades automatically. Tests: `documents.test.ts` (`markMissingOnDisk` set/clear/reference-stable), `App.test.ts` (desktop: delete an open file → tab struck + dock row gone + buffer kept → Save writes to the original path + strike clears). Frontend-only (Vite HMR; no `just dev` restart).
- *2026-06-28* — **T7.35 — Dock folder indent guides (Zed/VS-Code style).** Each folder's children now draw a faint vertical guide line down their left edge, terminating exactly where that folder's contents end, so folder width and nesting depth read at a glance. Pure `Dock.svelte` change on the existing recursive `row` snippet, no model change: the folder's nested `<ul class="file-list nested">` carries the folder's own `--depth`, and a `::before` pseudo-element draws the line absolutely-positioned `top:0; bottom:0` (so its height auto-fits the children block and ends at the last child), 1px wide in `--border`, offset to sit under the parent folder's icon at `calc(0.7rem + var(--depth) * 0.85rem + 0.45rem)` — reusing the same `0.85rem` indent unit the rows already use, so the guide stays aligned at every nesting level. Test in `Dock.test.ts` asserts each nesting level's `<ul>` carries its parent folder's depth (the guide's anchor: 0 then 1 for `licks/rolls/…`); the rendered line is visual-only (jsdom can't measure pseudo-elements). Frontend-only (Vite HMR; no `just dev` restart).
- *2026-06-28* — **T7.36 sub-chunk 2.1 — dock right-click context menu + inline-edit scaffolding (ops stubbed).** First slice of dock file management. **Decisions (user):** the menu is **desktop live-folder only** — shown when a real folder is open (`projectRoot` set), absent on web / lone-file / draft projects (web joins when the Chunk-D FSA folder lands); and names are typed via an **inline tree input** (VS Code/Zed feel), not a modal. **Built:** a reusable `ContextMenu.svelte` — `position: fixed` at the pointer's x/y (so the dock's `overflow` can't clip it, nudged back on-screen once measured), dismiss on Escape or an outside pointer-down (the New "+" popover pattern), with destructive-item colouring + separators. `Dock.svelte` gained five props — `canManage`, `pendingEdit`, `onContext`, `onCommitEdit`, `onCancelEdit`: right-clicking a row or empty space opens the menu (New File / New Folder always; Rename / Delete only off a real file or folder — a path-null **draft** row reports as `root`, so it offers only the New items), and a host-driven `pendingEdit` renders an **inline `<input>` inside the tree** — a phantom row inside the target folder (or root) for New File/Folder, or swapping a row's label for Rename, seeded with the current name. The input auto-focuses + selects, **auto-expands a collapsed target folder** so it's visible, commits on Enter behind a non-empty / no-path-separator guard, and cancels on Escape or blur. Shared `DockTarget` (`folder | file | root`) and `PendingEdit` (`new-file | new-folder | rename`) types live in `project.ts`. `App.svelte` owns the `pendingEdit` state and `onDockContext` (menu pick → set `pendingEdit`, or Delete → `askConfirm`/`ConfirmDialog`), `commitDockEdit`, `cancelDockEdit`, `deleteEntry`. The actual filesystem ops (create/rename/remove) are **stubbed to a console line** this chunk — they land in 2.2 (empty-folder scan support) → 2.3 (New/Delete + `fs:allow-mkdir`/`remove`) → 2.4 (Rename + `fs:allow-rename`, open-file-follows). Tests: `ContextMenu.test.ts` (items/separator/destructive, select, dismiss-outside/inside/Escape) and `Dock.test.ts` (menu gating by `canManage`, item set by target, `onContext` payload, inline new-file commit-on-Enter + empty/separator rejection + Escape-cancel, rename row-swap seeded with the name). Frontend-only (Vite HMR; no `just dev` restart). **Next: 2.2 — thread directory keys through `collectCtabFiles`/`reconcileScan`/`projectTree` so empty folders exist and survive rescans.**
- *2026-06-28* — **T7.36 sub-chunk 2.2 — empty-folder scan support (dirs through scan → reconcile → tree).** The dock tree was derived purely from file *paths*, so an empty folder (no `.ctab` inside) didn't exist and would vanish on rescan — blocking New Folder. Fixed by carrying directory keys end to end. `collectCtabFiles` now returns a third field `dirs`: the root-relative forward-slash key of every non-dot directory it walks into, **including empty ones** (a `FolderContents` interface is the shared return shape for `openFolder`/`rescanFolder`, both now emitting `dirs`). `watch.ts`'s `FolderScan` and `FolderReconcile` gained `dirs`, and `reconcileScan` passes it straight through. `projectTree(entries, dirs?)` gained an `ensureFolder(path)` helper that creates a folder node and its ancestors on demand and memoizes by full path — so a dir key materializes an empty folder, and a `licks/` that appears in both `dirs` and a file's path resolves to the **same** node (no duplication); dir keys are normalized (`\`→`/`, leading/trailing slashes and blanks dropped). `App.svelte` holds `projectDirs` (set on `openProjectInto`, the folder-open flow, and every `applyScan`) and threads it to the Dock as `dirs={projectDirs}` → `projectTree(entries, dirs)`. Non-folder projects (single score / bundle / draft) pass `[]`, unchanged. Tests: `io.test.ts` (dirs collected incl. an empty dir; dot-dir excluded; walk-order of the file map), `watch.test.ts` (dirs carried through reconcile), `project.test.ts` (empty-folder + ancestors materialize, the shared-node no-dup case, key normalization), `Dock.test.ts` (an empty folder renders from `dirs`), `App.test.ts` (desktop: a scan's empty `drafts/` dir renders in the dock, sorted before the root file). Pure model + frontend-only (Vite HMR; no `just dev` restart). **Next: 2.3 — the New File / New Folder / Delete fs ops behind the io seam + `fs:allow-mkdir`/`fs:allow-remove` capabilities (create opens the new file as a tab; delete closes the open tab; the watcher reconciles the dock).**
- *2026-06-28* — **T7.36 sub-chunk 2.3 — New File / New Folder / Delete against the live folder (+ fs capabilities).** The dock's right-click ops now mutate the real desktop folder. `io.ts` gained `resolvePath(root, key)` — the inverse of `toRelative`, rejoining a root-relative forward-slash key under `root` segment-by-segment with the platform separator (so a Windows root gets backslashes) — and three desktop-only fs ops, each a no-op resolving off-desktop: `createFile(path, content="")` (writeTextFile; New File seeds an empty `.ctab`), `createDir(path)` (mkdir `{recursive:true}` — idempotent, so re-creating an existing folder is harmless), and `removePath(path, recursive)` (remove; recursive for a folder). The capability set in `src-tauri/capabilities/default.json` gained **`fs:allow-mkdir`** and **`fs:allow-remove`** — unlike `watch`, these standard fs commands aren't behind a Cargo feature, so no `Cargo.toml` change was needed; but capabilities are baked into the Rust binary, so this needs a `just dev` restart (the prior live-folder GOTCHA: the fs-mocking Vitest suite can't catch a missing capability). `App.svelte`: `commitDockEdit` is now async — for New File/Folder it builds the key (`parentPath/leaf`, leaf `withCtabExtension`'d for files), `createFile`/`createDir`s at `resolvePath(root, key)`, updates `projectFiles`/`filePaths`/`projectDirs` **optimistically**, and (for a file) opens it as a tab via `addOrFocusFile` — a name collision with an existing file just focuses it rather than clobbering; failures `console.error` + `window.alert` (the latter a no-op in WKWebView, so the log is the real signal). `deleteEntry` confirms via `ConfirmDialog` (folder copy warns it deletes the contents), `removePath`s, then **force-closes** the affected tab(s) and drops their rows — a new guard-free `forceCloseDoc(docId)` closes every view of a doc and removes its session with no dirty-prompt (an explicit delete is the user's intent, distinct from the watcher's *missing-on-disk* striking for an *external* delete), and `omitKeys` prunes `projectFiles`/`filePaths` (plus a folder's whole subtree). The live-folder watcher re-scan then reconciles to the same state — creates/deletes already reflected, so it's idempotent with no clobber. Rename is still stubbed (→ 2.4). Tests: `io.test.ts` (`resolvePath` posix/windows, create/mkdir/remove call shapes, off-desktop no-ops) and `App.test.ts` (desktop: New File creates at the live folder + opens as a tab + lists in the dock; New Folder creates + renders empty; Delete removes + closes the tab + drops the dock row). **Must be verified in the real desktop app** (the capability grant can't be exercised by the mocked suite). **Next: 2.4 — Rename + `fs:allow-rename`, with an open file following the rename (doc id `file:<old>`→`file:<new>`, session key/path, workspace `docId`s, and the per-doc maps).**
- *2026-06-28* — **T7.36 sub-chunk 2.4 — Rename, with an open file's tab following the move (T7.36 complete).** The last dock op: Rename a file or folder on the live folder, re-keying the project and migrating any open document so its tab follows — buffer and dirty state intact. `io.ts` gained `renamePath(from, to)` (Tauri `rename`; desktop-only, web no-op) and the capability set gained **`fs:allow-rename`** (binary-baked → `just dev` restart; like mkdir/remove it isn't behind a Cargo feature). `App.svelte`: `commitDockEdit`'s rename branch calls `renameEntry(oldKey, isFolder, name, root)`, which builds the new key (a file's leaf is `withCtabExtension`'d, a folder's is bare; both keep the same parent dir), **collision-guards** against an existing sibling file/folder (alerts, aborts), `renamePath`s `resolvePath(root, old/new)`, then re-keys `projectFiles`, `filePaths` (values recomputed to the new abs path), and `projectDirs` — a **folder rename carries its whole subtree** via `isUnder(key, base)` / `reprefix(key, oldBase, newBase)` helpers (every descendant key re-prefixed). Each open file under the renamed key is migrated by `renameOpenDoc(oldKey, newKey, newAbs)`: the six reactive per-doc maps (`results`, `errors`, `selections`, `activeSpans`, `layoutWidths`, `loadRequests`) are carried over with a pure `renameKey`, the **live-compiler and edit-handler closures are dropped** (they captured the old id, so they're recreated lazily under the new id rather than migrated), the session and workspace are re-keyed, and the doc recompiles. Because the buffer lives in the session (preserved across the re-key), an **unsaved rename keeps its edits** and subsequently saves to the new path; the editor remounts (its instance id derives from the doc id) so in-editor undo resets — consistent with the watch-reload behavior. Two new pure model ops back this: `documents.ts` `renameDoc(store, oldId, newId, {name, path})` (re-key the session, preserve content/baseline/flags, active pointer follows) and `workspace.ts` `renameDoc(ws, oldId, newId)` (re-point every tab of the doc to a fresh instance and update each group's active id; the group-level maximize state is unaffected). The live-folder watcher's re-scan then reconciles to the same keys (the rename already reflected), idempotently. Tests: `io.test.ts` (`renamePath` call shape + web no-op), `documents.test.ts` (`renameDoc` re-keys + preserves dirty + no-op when absent), `workspace.test.ts` (`renameDoc` re-points views + active id, leaves other docs alone), `App.test.ts` (desktop: Rename moves on disk, relabels the dock row, and the open editor tab follows showing the same buffer). **Must be verified in the real desktop app** (the `fs:allow-rename` grant can't be exercised by the fs-mocking suite). **T7.36 is complete — the dock's New File / New Folder / Rename / Delete all work against the live folder, with the watcher reconciling and open tabs following deletes/renames.** Only Chunk D (optional web-FSA directory open) remains open under the dock/folder umbrella (T7.15).
- *2026-06-28* — **T7.15 closed — Chunk D (web File System Access folder open) skipped by decision.** The desktop live folder (Chunks A–C: Tauri open/write-back/watch) plus T7.36 (dock New File/Folder/Rename/Delete) are complete, so the only open piece was the optional Chunk D — opening a real local *directory* in the browser via the Chromium-only File System Access API, snapshot + a manual Refresh (FSA has no change-notification API, so no live watch). Decision (user): **skip it.** Rationale — a **desktop app should have desktop-like features** (a live, watched folder) and a **web app web-like features** (the portable `.ctabz` bundle); that division is an implied, fair UX expectation rather than a gap. Per D38 FSA was always an optional Chromium-only enhancement, **not** the dependency: the bundle already gives web users complete multi-file projects on *every* browser (Firefox/Safari included, which lack the directory picker), so nothing is blocked. Building Chunk D would add a Chromium-only, degraded third sync model to maintain (desktop watch · web-FSA refresh · bundle snapshot) and — to reach parity with the file management just shipped — require reimplementing create/rename/delete against FSA handles (FSA has no clean rename; needs `move()` or read+write+delete) plus permission re-prompts and IndexedDB handle persistence. Revisit post-MVP only if browser-side folder *authoring* (vs. viewing/sharing) becomes a real target. **The web baseline stays the bundle; the live folder stays desktop-only.**
- *2026-06-28* — **T7.17a — pin the page width to the layout target.** `layout()` now sets the page width to `overall_width(...).max(config.width)` — the page is the layout target (viewport/export width), no longer the content-derived widest-system extent. So the centred title/composer stay anchored at the true page midpoint and the viewBox stops shrinking as a short score is edited; `overall_width` still wins only when a single measure overflows the target (it can't be clipped). This generalizes the def-gallery's existing pin (`width = config.width`) to the main path and is the prerequisite for justifying systems (T7.17b): the justify pass needs a fixed target to stretch each system to. Layout-only, no model/serializer change; `overall_width`'s doc note now frames it as the content floor the caller pins up. Tests: a lone short measure pins to the 800-unit target; a measure packed past a tiny 10-unit target grows past it. Snapshots re-accepted across `layout.rs` (meta.width, system `bounds.w`, centred-header `x`) and `compile_wire_format` — all width/centering shifts only, fret/content positions unchanged; the gallery wire-format snapshot was already pinned and didn't move. Ran `just wasm` so the web app picks up the core change. **Next: T7.17b — justify (stretch) each system's measures/events to fill the pinned page width.**
- *2026-06-28* — **T7.17b — justify systems to fill the pinned page (T7.17 complete).** `build_system` now stretches each system's measures to fill the width T7.17a pinned. A per-system `scale = (width - LEFT_MARGIN - RIGHT_MARGIN) / Σ measure widths`, clamped `>= 1` (never compress), multiplies the *base* positions only — each measure's advance (`mwidth = plan.width * scale`, also the `MeasureBox.bounds.w` so hit-testing matches the rendered span) and every event onset (`rel_x * scale`, threaded through fret numbers, stems, beam member x's, lone-flag x, and tie `next_x`). Glyph-relative offsets stay **unscaled** — augmentation dots, technique marks, beam overhang/flag length, the string-line number gaps, and the time-signature column (`tsx`) — so stretching adds air between notes without smearing the glyphs themselves; engraving-wise the time sig is fixed-width and the spring after it grows. Because scale is clamped `>= 1`, the widest system (which pins the page) sits at scale 1 untouched, and every shorter system — including a sparse final line — stretches so its last barline lands at `width - RIGHT_MARGIN` (verified across the wrapped/repeat snapshots: every system's max line-x equals its target). A `justify: bool` param gates it: `layout()` passes `true`; **`layout_gallery()` passes `false`** — a short sample lick in a def-gallery card should hug the left at natural width, not smear across the page (so the gallery snapshots are unchanged). **Provisional (per iterate-on-feel):** justification is uniform with no stretch cap, so a lone-measure final line spreads wide; if that reads as over-spaced in use, a max-scale cap (fill *some* then left-align the remainder) is a cheap follow-on. Test fallout: five existing single-measure tests asserted *natural* (unstretched) intra-measure geometry on the wide default config and now saw the stretch — `spacing_is_time_proportional`/`a_rest_breaks_a_beam` moved to a new `cfg_natural()` (a 1-unit target so a real measure overflows → the page pins to its own content → scale 1, and coordinates stay small: a justified flag's `x2 - x1` was losing float precision in the hundreds, tripping the beams-vs-flags length check); `meter_changes…`/`sub_eighth…`/`a_time_signature_reserves_leading_width` are now asserted on `plan_measure` directly (the pre-justification plan is where intra-measure spacing actually lives); `the_first_note_clears_the_time_signature` relaxed its note-position check to `>=` (justification only ever pushes the note further from the fixed time-sig glyph). New tests: a short system's final barline reaches `width - RIGHT_MARGIN`; justification preserves the 2:1 half-vs-quarter onset ratio while genuinely widening the gap; the page-pinning widest system stays at scale 1. Snapshots re-accepted (layout positions scaled; gallery wire-format unchanged). Ran `just wasm`. **Next: T7.18 — even out intra-measure spacing (trailing-space vs leading-pad), which pairs with this justify pass.**
- *2026-06-28* — **T7.18 — intra-measure spacing reviewed; kept as-is by decision (no code change).** Confirmed first that T7.17b's justification did **not** address this and orthogonally amplifies it: justify scales the whole measure by one factor, preserving the internal trailing-vs-leading asymmetry while pinning the right barline so the last note's gap reads larger. The asymmetry lives in `plan_measure`: an event is placed at its onset `x`, then `x` advances by that event's *own* duration before the next, and after the **last** note it still advances a full duration before `+ MEASURE_PAD` — so the first note has only `MEASURE_PAD` (~0.8) of leading space while the last note has `its full duration + MEASURE_PAD` of trailing space (a lone whole note sits jammed at the far left with ~8 units of emptiness to the right barline). Three options were weighed: **(a) center notes in their duration-slots** — symmetric edges, a lone whole note centers, and crucially measure widths / line-breaking / justification stay identical (only glyph x shifts; equal-duration gaps unchanged), at the cost of inter-note gaps becoming the *average* of neighboring durations rather than literally following the leading note's duration; **(b) cap the last note's trailing space at `MEASURE_PAD`** — symmetric edges but a long-note-ending measure becomes narrower than its duration implies (cramped within a justified line; changes relative widths / re-breaks); **(c) leave as-is.** **Decision (user): leave as-is.** Rationale — onset-based "space follows the note's duration" *is* standard professional engraving (a long note earns space after it); the perceived imbalance is intentional, not a bug. T7.18 closed as reviewed/won't-fix; no change to `plan_measure` or the spacing constants. If a lone-whole-note bar ever reads as too left-jammed in practice, option (a) (slot-centering) is the cheap, low-risk revisit since it leaves all widths intact.
- *2026-06-28* — **T7.19a — pagination layout (core; the layout half of T7.19-PDF).** New `paginate(score, PageConfig) -> PaginatedTree` in `layout.rs` packs the same justified systems `layout()` produces onto fixed-size pages, breaking to a new page when the next system's footprint would cross the bottom margin. **Built natively, not as a reflow:** rather than laying out one tall tree and translating systems onto pages (which would mean rewriting the absolute coords baked into `Primitive::Path` strings — ties/slides are paths), the prep is factored out (`prepare()` → measure plans, beam groups, bar numbers, system groupings, pinned width) and shared by both paths; `layout()` and `paginate()` then differ only in how they stack — one continuous, one page-broken — calling `build_system` with **page-relative** y so each page is its own `(0,0)` coordinate space the painter draws like any tree. New render types `Page { bounds, header, systems }` and `PaginatedTree { page_width, page_height, pages }` (render.rs, serde camelCase, round-trip tested). **Page geometry:** `PageSize::{Letter,A4}` contributes only a portrait *aspect ratio* — pagination never works in physical units (the exporter picks DPI). The page is `prep.width` wide (content already inset by LEFT/RIGHT_MARGIN, same as screen) and `width * (h_in/w_in)` tall; the vertical content band is `[TOP_MARGIN, page_height − BOTTOM_MARGIN]`. **Per-page header = folio numbers (chosen over running-head / none):** page one carries the full title block (built from the top margin via a new `top` param on `build_header`); continuation pages carry a small `TextRole::PageNumber` top-right (1-based, page one omits it), clearing a `FOLIO_SPACE` band before their first system. Packing always places ≥1 system per page (mirrors `pack_systems` horizontally); an empty score still emits one title-only page (never blank, per D31). Tests (12 new): one-page/multi-page/empty counts; the page-break golden — every non-last page is *full* (re-placing the next page's first system after the last would overflow the bottom limit); pagination preserves every system in order vs. `layout()` (no drop/dupe/reorder, by measure-span sequence); systems stay within page bounds; folio presence/numbering and title-block-on-page-one; Letter aspect + shared page box; A4 taller than Letter at equal width. **No frontend yet:** `TextRole::PageNumber` is added to the Rust enum and the render round-trip role list, but the TS mirror (`types.ts`), `tabStyle.ts` styling, the SVG/PDF serializer, and the wasm `paginate` binding are **T7.19b** (PDF emission) — `paginate` is core-only here. **Provisional (per iterate-on-feel):** the page margins reuse the existing screen constants (LEFT/RIGHT/TOP/BOTTOM_MARGIN) rather than a true print margin (e.g. 0.75 in), so content sits close to the page edge; if that reads as cramped once a PDF is actually rendered (T7.19b), widening the horizontal inset needs either larger print-path margin constants or a per-page content offset (which reintroduces the path-translate problem, so prefer the former). **Next: T7.19b — serialize `PaginatedTree` to PDF bytes (one page → one PDF page), styling `PageNumber`, and expose `paginate` over wasm.**
- *2026-06-28* — **T7.19b — vector PDF emission (crisp, font-embedded; the serializer half of T7.19-PDF).** Turns a `PaginatedTree` into PDF **bytes** as true vector content — real text + stroked paths, crisp at any zoom, small files (a 2-page tune is ~33 KB), identical on desktop (WKWebView) and web. **Chosen over rasterizing** (user priority: a stellar, sharp UI) despite the heavier lift: the render emits glyphs absent from the standard-14 PDF fonts (circled tuning digits `①`, tempo `♩`, musical rests U+1D13B–40, strum arrows `↓↑`), so vector requires embedding fonts. **Seam (mirrors `compile` end-to-end):** new `paginate_with_provider(source, PageConfig, provider)` in core (parse → imports → eval → `paginate`; a library wraps its def-gallery as one content-sized page), exposed as a wasm `paginate` binding and a Tauri `paginate` command, dispatched by a new `core.ts` `paginate()` seam; `types.ts` mirrors `Page`/`PaginatedTree`/`PageConfig`/`PageSize` and adds `pageNumber` to `TextRole`. **Painter (`app/src/lib/pdf.ts`, vector via `pdf-lib` + `@pdf-lib/fontkit`, both dynamically imported so they never touch the initial bundle):** walks each page's header + system + measure primitives, reusing `tabStyle.ts` (`TEXT_STYLE`/`textAnchor`/`isMuted`/`PATH_STROKE_WIDTH`) as the one styling source of truth so the PDF matches the screen. PDF is y-up from bottom-left, so a uniform `scale = points/page.bounds.w` maps the logical box onto the physical point size (Letter 612×792 / A4 595×842) and every y flips; `Line`→`drawLine`, `Path`→`drawSvgPath` (its SVG-space origin placed at the page top-left, **stroked** not filled), `Text`→`drawText` with anchor (`start`/`middle`/`end`) + a cap-height central-baseline offset matching the SVG painter's `dominant-baseline: central`. **Special glyphs:** musical rests + the tempo `♩` come from an embedded **Noto Music** subset; circled tuning digits render as a **vector circle + plain digit** and strum arrows as a **vector chevron** (font-independent, crisper, and avoids enclosed-alphanumerics/arrow font coverage); mixed-content roles (`tuningString` "①=D", `tempo` "♩ = 120") split the leading glyph from the serif remainder. **Typography unified (user choice):** the on-screen tab now renders in self-hosted **Source Serif 4** (woff2 `@font-face` in `app.css`, the same family embedded/subset as woff into the PDF — fontkit decodes WOFF1, not WOFF2's Brotli), so screen and print are one identical face; `FONT_FAMILY` (tabStyle.ts) + `Tab.svelte` point at it, and `svg.ts`'s `font-family` attribute switched to double quotes since the family name is single-quoted. The render *tree* is font-independent (layout never measures text), so **no Rust snapshot churn**. Fonts vendored into `public/fonts/` (OFL, see `OFL-NOTICE.md`) sourced via `@fontsource/{source-serif-4,noto-music}`; a folio `pageNumber` style (small, muted, end-anchored) added to `tabStyle.ts`. **Verified visually**, not just by assert: rendered the showcase + a 2-page synthetic to PDF and rasterized — crisp serif text, vector tuning circles, the `♩`, rests, ties/bends as clean stroked paths, beams/fingerings, and the top-right folio on page 2 all land; `drawSvgPath` confirmed to stroke (not fill) open paths. Tests: `pdf.test.ts` (jsdom, real vendored fonts via a `fetch` stub) asserts the task's bar — bytes start `%PDF-`, end `%%EOF`, page count matches the tree — plus glyph-fallback coverage (`pdf.ts` 99% lines); a wasm `paginate_marshals_a_paginated_tree` + core `paginates_a_score/library` round-trips. New deps: `pdf-lib`, `@pdf-lib/fontkit`, `@fontsource/*` (deps, like `material-symbols`), `@types/node` (dev; opted into the one test via a file-local triple-slash ref so browser source keeps its jsdom purity). **Provisional / not done:** screen still renders the musical glyphs (rests/`♩`/`①`/arrows) via system fonts while the PDF uses Noto Music/vector — full screen↔PDF parity (a music webfont on screen, or core emitting these as vector primitives) is a clean follow-up; the bend technique mark draws a tight little hook that slightly overlaps its fret number (pre-existing core geometry, visible on screen too, not a PDF artifact); `PageSize` defaults to Letter and the export content-width constant lives with the caller in T7.19c. **Next: T7.19c — save the bytes through the io seam (binary write on desktop, download on web) behind an export action.**
- *2026-06-28* — **T7.19c — save PDF through the io seam + export action (T7.19 complete).** Wires the paginate→paint chain to a user-reachable control. `io.ts` gains `savePdf(bytes, target)` — the same binary seam as `savePng` (Tauri `writeFile` on desktop, a `Blob`-download on web), swapping in the `.pdf` extension/filter. `App.svelte` gains an `exportPdf()` action behind the existing topbar **Export** menu (now SVG / PNG / **PDF** / Bundle): it `paginate(source, {size:"letter", contentWidth: PDF_CONTENT_WIDTH=80}, ctx)` through the core seam (same project context as `compile`), dynamically imports `pdf.ts` to paint the bytes, then `savePdf`s them — so the heavy `pdf-lib`/`fontkit` only load on first export (the production build code-splits them into export-only chunks: `pdf` 3 KB + `fontkit` 716 KB + pdf-lib, none in the initial bundle). Exports stay derived artifacts (always `path: null` → prompt/download, never rebaselining the doc). `PDF_CONTENT_WIDTH` is provisional (the 80-unit value eyeballed in T7.19b). Tests: `io.test.ts` covers `savePdf` on both desktop (binary writer) and web (download, `.pdf` extension); `App.test.ts` drives the menu end-to-end (Export → Export PDF → paginate mock → pdf mock → `savePdf`), asserting bytes + the suggested name. Full gate green; coverage io.ts 98%, pdf.ts 99%, overall 97%. **PDF export now ships end to end (D30 satisfied): SVG + PNG + paginated vector PDF, on desktop and web.** Manual click-test in the running app is the remaining acceptance step. **Next: T7.20 — fold the four export buttons into one unified Export control with a format picker (D48), pairing with the cohesion pass (T7.34).**
- *2026-06-28* — **T7.20 — unified export control: already satisfied, closed with no code change.** Verified the topbar **Export** dropdown is already the single, unified control D48 calls for: one download-icon button (`aria-haspopup="menu"`) opening one menu of SVG / PNG / PDF (+ project Bundle) items, every format routed through the existing io seam (`saveSvg`/`savePng`/`savePdf`, binary write on desktop / download on web). Git arc confirms the fold: M5 (`0c117b8`) shipped **separate** top-level `Export SVG` / `Export PNG` buttons; the iconify pass (`c070d3f`) folded them into the dropdown; **T7.19c** added the PDF item — completing the SVG/PNG/PDF picker the task specifies. No leftover per-format buttons exist anywhere in the UI. Closed as done (cf. the T7.18 "reviewed, no code change" precedent); the broader D48 "one command source shared with the native desktop menu" remains its own task (T7.30). **Next: T7.21 — dark theme by default.**
- *2026-06-28* — **PDF export polish (feedback on T7.19).** Four refinements from using the PDF export. **(1) Filename:** exports swap the source `.ctab`/`.ctabz` extension for the artifact's (`tune.pdf`, not `tune.ctab.pdf`) — the desktop path previously seeded the save dialog with the unswapped `.ctab` name, so the PDF filter appended `.pdf`. **(2) No prompt → Downloads:** the three render exports (SVG/PNG/PDF) now write straight to the OS Downloads folder with no dialog — a derived artifact is a throwaway, not a document you pick a home for. `io.ts` `saveSvg/savePng/savePdf` were unified behind a private `saveExport(bytes, suggestedName, ext, mime)`: desktop resolves `downloadDir()` (within the app's existing `$HOME/**` fs scope) + `join` + `writeFile`; web keeps the anchor download. Signatures simplified from `SaveTarget` → a plain `suggestedName` string (exports never overwrite a known path). Document save + project Bundle export still prompt (those are your real files). The old `writeBinaryTauri` (dialog-based) is gone. **(3) Success indication:** since exports now skip the dialog, the bottom bar flashes "✓ Exported <name>" for ~3s (fades in; takes the diagnostics slot, then clears) — `BottomBar` gained a `notice` prop, `App.svelte` a `notifyExport` helper called with each export's resolved name. **(4) Print margins (PDF only, `paginate`):** the page now gives the **title** top breathing room (`PRINT_MARGIN_TOP` 7 units ≈ 0.74in, was the screen's 0.5) and **indents the header's left block** (tuning name / string grid / capo) in from the edge by `PRINT_HEADER_INDENT` (4 units) — while the **staff/stanzas keep their on-screen left position** (user's call: don't move the staff). Mechanically, `build_header` took a `left_indent` param that shifts only the left-anchored items (the title/composer/tempo stay centred on the frame); the page stays the content-frame width, so the staff is unmoved and only the header and top inset change. The screen render (`layout`) passes `left_indent = 0` and is byte-identical (snapshots unchanged). Values are provisional (eyeballed on a rendered Letter page). Tests: `io.test.ts` covers `savePdf`/`saveSvg`/`savePng` writing to Downloads (desktop) + downloading with the swapped extension (web); `App.test.ts` drives Export → menu → notice for SVG/PDF; coverage io.ts 100%, BottomBar 100%, pdf.ts 99%. Full gate green; `just wasm` rebuilt. **Verified visually** by rendering the showcase to PDF and rasterizing — title spaced, header indented past the staff's (unmoved) left edge, stanza spacing intact.
- *2026-06-28* — **PDF/render header polish (feedback).** Two small `tabStyle.ts` tweaks (shared, so screen + PDF both): the **header block** (tuning name, string grid, capo) drops out of `MUTED_ROLES` → renders in **full black ink** instead of secondary grey (it was reading as washed-out on the printed page; technique/finger/strum/bar-number/folio marks stay muted). And the **title/composer/header sizes bump up** — title 1.5→1.8, composer 1.0→1.2, tuning/tempo/capo 0.85–0.9→1.0 — still within the layout's reserved row heights (TITLE_H/COMPOSER_H/META_LINE_H), so no positions move and the core/layout is untouched. Two unit tests repinned (`svg.test.ts` tuning-block now asserts ink not `#6b6b6b`; `Tab.test.ts` title font-size 1.8). Frontend gate green (310 tests).
- *2026-06-28* — **Header type scaled up + row heights to match (feedback, supersedes the sizes in the prior two entries).** Final header type (tabStyle.ts, screen + PDF): title **2.2**, composer **1.5**, tuning name / string grid / tempo / capo **1.2**, bar numbers **1.0** (all full-black; the header block is no longer muted). Because the bigger type was crowding its rows, the core layout reserves more vertical room: `TITLE_H` 2.0→2.8, `COMPOSER_H` 1.2→1.9, `META_LINE_H` 1.0→1.5, `BARNUM_SPACE` 0.9→1.3, and `TUNING_COL_W` 2.8→3.4 (wider grid cells for the larger `①=D`). Layout-only (row reservations); fret/staff geometry unchanged. Snapshots re-accepted (9 `layout` + `compile_wire_format` — all header/band y-positions shifted down by the larger reservations; staff content unchanged) and `just wasm` rebuilt so the web render picks up the spacing. Full gate green.
- *2026-06-28* — **Print preview shows the real paginated layout; tempo note enlarged (feedback).** **(1) Preview = print output.** `PreviewView` previously rendered the *screen* render tree (`renderTreeToSvg(result.renderTree)`), so it didn't show the PDF's top margin, header indent, or page breaks — dishonest about what prints. It now **paginates its own source** through the core seam (`paginate(source, {size:"letter", contentWidth: PDF_CONTENT_WIDTH}, ctx)`, debounced 150ms, latest-wins) and renders **each page** as a light sheet via a new `renderPageToSvg(page)` (svg.ts, factored to share the body/wrap with `renderTreeToSvg`), stacked down the pane. So the preview is now pixel-faithful to the exported PDF (same pagination + margins), not the reflowed editor render. App feeds it the doc's `source`/`basePath`/`files` instead of the compiled result; `PDF_CONTENT_WIDTH` moved to `sizing.ts` so the preview and the PDF export share one density constant. **(2) Tempo note bigger.** The metronome mark's leading ♩ renders small for its em (and smaller still in Noto Music), so both painters now draw it at `TEMPO_NOTE_BOOST` (1.6×) the tempo text size, each piece cap-centred on the same y: a larger `<tspan>` in svg.ts (screen preview / SVG export) and a boosted `drawText` in pdf.ts. Left the live render view (`Tab.svelte`) unchanged — it's the working editor view, not the print output. Tests: `PreviewView.test.ts` rewritten to mock the core `paginate` and assert the page sheets render (Letter); `svg.test.ts` covers the tempo `<tspan>` boost + `renderPageToSvg`; App's preview test still green via the existing wasm `paginate` mock. Coverage svg/sizing/tabStyle 100%, pdf 99%, overall 97%. Full gate green (TS-only; no core/wasm change).
