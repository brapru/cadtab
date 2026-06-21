# cadtab — Grammar (living)

> The reference grammar for the cadtab DSL. **Living and incremental** (T1.0): the confident
> core is pinned here now; uncertain constructs are marked **`[PROVISIONAL]`** and settled
> construct-by-construct *alongside* the lexer (T1.2) and parser (T1.4) — not all up front.
> Design rationale lives in [`DESIGN.md`](./DESIGN.md) (referenced as D#); the canonical surface
> examples are §6 (core syntax) and §7 (licks/functions).
>
> **Status:** this is a *test-oracle*, not a one-way door. The lexer/parser snapshot corpora
> (`insta`) are the executable form of this document; when they disagree, one of them is wrong —
> fix it and update both. Keep this file and the corpora in lock-step.

## Notation

EBNF-ish, with these conventions:

- `=` defines a rule; rules are `lower_snake_case`.
- Terminals are `"quoted"` (literal text) or `UPPER_CASE` (token classes from the lexer).
- `{ x }` = zero-or-more, `[ x ]` = optional, `( … )` = grouping, `a | b` = alternation.
- `x+` = one-or-more. Juxtaposition = sequence.
- Whitespace (incl. newlines) separates tokens but is otherwise insignificant (D10) — free
  formatting. Comments are whitespace.

Maturity tags:

- **`[CORE]`** — confident; matches the §6/§7 frozen design. Will get snapshot coverage first.
- **`[PROVISIONAL]`** — shape expected but not yet pinned; finalized when its construct is built.

---

## 1. Lexical grammar (tokens) — `[CORE]`

The lexer (T1.2) is hand-rolled (D18, D40), emits **classified** tokens + spans (for
highlighting *and* diagnostics, D27), and never bails (error tokens, D19).

```
INT     = DIGIT { DIGIT }                          // unsigned decimal
STRING  = '"' { CHAR_NO_DQ_NL | '\\' ANY } '"'     // double-quoted, single-line, backslash escapes
IDENT   = ALPHA { ALPHA | DIGIT | ("_" ALPHA) }    // no leading "_"; "_" joins only before a letter
COMMENT = "//" { ANY_NO_NL }                       // line comment        (whitespace)
        | "/*" { ANY } "*/"                         // block comment       (whitespace; non-nesting [CORE])
WS      = SP | TAB | NL | CR                        // insignificant
```

Punctuation / operator tokens:

```
":"  ".."  "..."  "."  "_"  "~"  "/"  "["  "]"  "{"  "}"  "("  ")"  ","  "="
```

- `"..."` (spread) is scanned greedily and must out-prioritize `".."`/`"."` (maximal munch).
  `".."` is reserved (no current use) so a stray `..` lexes as one token, not two `.`.
- `"_"` is always the duration-suffix lead token (`Underscore`). Identifiers may *contain* `_`
  (`forward_roll`) but never *start* with one, so `_8` lexes as `Underscore` `Int`, never as an
  identifier. Inside a name, `_` joins only when followed by a **letter**; `_` before a digit is a
  duration, so `r_8` lexes as `r` then `_8` (rest + eighth) and a name cannot contain `_<digit>`.
- Strings are **single-line**: a newline (or EOF) before the closing `"` is an unterminated-string
  diagnostic; the `\`-escape is scanned but not decoded by the lexer.
- `"."` carries three surface meanings disambiguated by what follows (parser, not lexer):
  right-hand **mark** (`.` + mark-letter), phrase **index** (`.` + `INT`), and is otherwise a
  bare dot. The lexer emits a single `.` token; classification happens in the parser.

### Keywords — `[CORE]`

Recognized from `IDENT` after scanning (a keyword is never also an identifier):

```
title  composer  tempo  instrument  tuning  capo  import
score  time  default  pickup  repeat  ending  loop  measure
def  let
```

- `r` (rest, §3) is lexed as `IDENT` and recognized as a **rest** only in event position; it is
  not a reserved keyword elsewhere (so `r` could name nothing today, but the grammar treats a
  bare `r`/`r_N` in an event slot as a rest — see §3). **`[PROVISIONAL]`**: may be promoted to a
  reserved keyword if collisions arise.
- Technique/utility functions (`hammer`, `pull`, `slide`, `bend`, `choke`, `ghost`, `len`) are
  **ordinary identifiers** resolved by name (D8, D17), **not** keywords.
- Mark letters `t i m d u` are not reserved globally — they are only special immediately after a
  `.` in note/chord context (§4).

---

## 2. Program structure — `[CORE]`

```
program     = { top_item }
top_item    = metadata_decl
            | instr_decl
            | tuning_decl
            | capo_decl
            | import_decl
            | def_decl
            | let_decl
            | score_block

metadata_decl = "title"    STRING        // D34
              | "composer" STRING
              | "tempo"    INT
instr_decl   = "instrument" IDENT        // builtin name: banjo | guitar (D35)
tuning_decl  = "tuning"     IDENT        // named tuning override (D35)
capo_decl    = "capo"       STRING       // display-only header label (D6)
import_decl  = "import"     STRING       // path / module (D29, D38)
```

Ordering is **free** at the top level (declarations may precede or follow `def`/`let`); a single
`score` block is the usual case but is not syntactically required to be unique (semantic check,
M2). Resolution/validation of these is M2's job — the grammar only fixes shape.

### `def` / `let` — `[CORE]`

```
def_decl = "def" IDENT "(" [ params ] ")" block      // body evaluates to a Phrase (D14)
params   = IDENT { "," IDENT }
let_decl = "let" IDENT "=" expr                       // e.g. let g_chord = [3:0 2:0 1:0]
block    = "{" { event } "}"
```

---

## 3. Score block & musical content — `[CORE]`

```
score_block = "score" "{" { score_item } "}"

score_item  = setting
            | pickup_block
            | repeat_block
            | loop_block
            | measure_block
            | event

setting     = "time"    INT "/" INT      // meter; may recur (meter change) (D12, D32)
            | "default" INT "/" INT      // sticky default duration as a fraction, e.g. 1/8 (D11)

pickup_block  = "pickup"  block                       // anacrusis, excluded from barring (D33)
loop_block    = "loop" INT block                      // unroll N copies (D32; renamed from repeat N)
measure_block = "measure" block                       // explicit bar override (D12)

repeat_block  = "repeat" "{" { event } { ending } "}" // musical repeat (D32)
ending        = "ending" "(" INT ")" block            // volta; ending(k) plays on pass k
```

- Body events of a `repeat` come **before** the first `ending`; endings are the trailing voltas
  (D32). Mixed order is an error-recovery case (T1.4g), not valid grammar.
- `loop INT` is the programmatic unroll (D16/D32); `repeat { … }` is the *musical* repeat. Do not
  conflate.

### Events — `[CORE]`

```
event     = tie_expr

tie_expr  = unit { "~" unit }            // tie operator (D36); left-assoc, see §5
unit      = note
          | chord
          | rest
          | call                          // technique/lick calls splice phrases (D8, D14)

note      = position [ mark ] [ duration ]            // string:fret(.mark)(_dur)  (D10)
position  = INT ":" INT                               // string : fret  (1-based string, D37)

chord     = "[" chord_note+ "]" [ duration ]         // pinch: ONE shared duration (D39)
chord_note = position [ mark ]                        // per-member right-hand mark only

rest      = "r" [ duration ]                          // r / r_N  (D39)

mark      = "." mark_kind
mark_kind = "t" | "i" | "m"              // right-hand fingers: Thumb/Index/Middle (D7)
          | "d" | "u"                    // strum Down / Up                      (D7)
```

### Durations — `_dur` suffix (D11) — mixed maturity

```
duration  = "_" dur_body
dur_body  = INT { "." }                  // [CORE]  denominator + dotted dots: _4, _8, _16.
          | tuplet                       // [PROVISIONAL] — settled in T1.2d (see below)
```

- **`[CORE]`** `_N` = note worth `1/N` of a whole note; `N` is the denominator (`_4` quarter,
  `_8` eighth, `_16` sixteenth). Trailing `.` = dotted (`_4.` dotted quarter); multiple dots
  allowed syntactically (`_4..`), semantics validated in M2.
- Omitted ⇒ inherit the sticky default (D11); `default` seeds it.
- **`[PROVISIONAL]` tuplet syntax** — the D11 TBD. **Not pinned in T1.0 by design.** Candidate
  forms (decide in T1.2d when the construct is actually lexed/parsed):
  - `_8t` (triplet marker letter appended), or
  - `_3:8` (count:base), or
  - a `tuplet(n) { … }` block.
  Until chosen, tuplets are **not accepted** by the lexer/parser and the corpora must not assume a
  form. Whichever wins updates this rule and the precedence table (§5).

---

## 4. Expressions — `[CORE]` (Pratt-parsed, D18)

Used in `let` RHS and call arguments (`forward_roll(g_chord)`, `hammer(3:0, 3:2)`,
`len(phrase)`, `forward_roll(...g_chord)`).

```
expr      = spread_expr
spread_expr = [ "..." ] tie_expr_e                   // spread only in argument position (D17)

postfix   = primary { postfix_op }
postfix_op = "." INT                     // phrase index .N            (D17)
           | "(" [ args ] ")"            // call
                                         // NB: "." mark_kind is NOT an expression postfix —
                                         // a mark attaches at the note/event level (§3), not as
                                         // a value operator. `chord.0 .t` = a note whose head is
                                         // the index expr `chord.0`, annotated with mark `.t`.

primary   = INT
          | STRING
          | IDENT
          | position                     // note literal as a value
          | chord                        // chord literal as a value
          | "(" expr ")"

args      = arg { "," arg }
arg       = spread_expr
```

- `position` (`string:fret`) is a **primary** (it binds tightest), so `3:2.t` parses as
  `(3:2).t`, never `3:(2.t)`.
- `len(x)` and technique functions are just `IDENT` + call `postfix_op`; no special grammar.
- `.N` (index) vs `.mark` (mark) are distinguished by the token after `.`: an `INT` ⇒ index, a
  mark-letter `IDENT` ⇒ mark. A `.` followed by anything else is a diagnostic (T1.4g).
- `...` (spread) appears only at the head of an `arg` / event `unit` head (splat a phrase into
  positional args, D17). `...g_chord.0` = `...(g_chord.0)` (spread is loosest, §5).

> **Note (event vs. expression contexts).** Inside a `score`/`def` block, content is parsed as
> **events** (§3): juxtaposed notes/chords/rests/calls. Inside `let`/argument contexts it is
> **expressions** (§4). The two share `position`, `chord`, `call`, index, mark, and spread; they
> differ in that durations and tie attach in event context, while value-producing operators
> (index, `len`, spread) dominate expression context. The parser selects the context by position.

---

## 5. Precedence & associativity — `[CORE]` (tuplet row provisional)

Listed **tightest-binding first** (level 1 binds most tightly). This is the authoritative
ordering the Pratt/recursive-descent parser (T1.4) implements.

| Lvl | Construct            | Form               | Fixity / assoc      | Notes |
|----:|----------------------|--------------------|---------------------|-------|
| 1   | note literal         | `string:fret`      | primary             | `:` binds tighter than any `.` (D10, D37) |
| 2   | call · index         | `f(a)` · `e.N`     | postfix, **left**   | chained L→R: `chord.0` = `(chord).0` (D8/D17) |
| 2′  | mark                 | `e.t`              | note-level suffix   | not a value op: attaches to the note whose head is `e` (§3/§4) |
| 3   | duration suffix      | `e_N`, `e_N.`      | postfix             | binds looser than `.`: `3:2.t_4` = `((3:2).t)_4` (D11) |
| 3′  | *tuplet marker*      | *TBD*              | postfix **`[PROVISIONAL]`** | co-located with `_dur`; pinned in T1.2d |
| 4   | tie                  | `a ~ b`            | infix, **left**     | looser than `_dur`: `3:2_4 ~ 3:2_8` ties two full notes (D36) |
| 5   | spread               | `...e`             | prefix (arg head)   | loosest: `...g_chord.0` = `...(g_chord.0)` (D17) |

Worked examples (the parser must agree with these):

```
3:2.t_4         => dur( mark( note(3,2), Thumb ), 1/4 )
chord.0.t       => mark( index( ident chord, 0 ), Thumb )
3:2_4 ~ 3:2_8   => tie( dur(note(3,2),1/4), dur(note(3,2),1/8) )
[1:0.m 5:0.t]_4 => dur( chord[ (1,0).m, (5,0).t ], 1/4 )
...g_chord.0    => spread( index( ident g_chord, 0 ) )
forward_roll(...g_chord)  => call( forward_roll, [ spread(ident g_chord) ] )
```

---

## 6. Worked corpus anchor — §6 "Cripple Creek" — `[CORE]`

The §6 program is the canonical end-to-end fixture (TASKS.md testing strategy). Every construct
it uses is `[CORE]` above:

- metadata (`title`/`composer`/`tempo`), `instrument`/`tuning`/`capo`
- `score { time 4/4  default 1/8 … }`
- `pickup { 2:0.i 1:0.t }`
- `repeat { … [1:0.m 5:0.t]_4  ending(1){ r_8 3:2 ~ 3:2 }  ending(2){ … } }`
- `loop 2 { 3:2 3:4 }`
- `measure { hammer(3:0, 3:2)  bend(1:7) }`

§7 adds: `def forward_roll(chord) { chord.0 .t … }`, `let g_chord = [3:0 2:0 1:0]`, calls,
`loop 3 { … }`, and spread `forward_roll(...g_chord)`.

> A `mark` written with surrounding space (`chord.0 .t`, as in §7) is the same `.t` postfix;
> whitespace before `.` is insignificant (§1). **`[PROVISIONAL]`**: confirm the parser treats
> ` .t` identically to `.t` when T1.4e lands; if a space must *not* separate a mark from its
> target, this note tightens.

---

## 7. Open items (resolve in lock-step with T1.2 / T1.4)

- **Tuplet syntax** (D11 TBD) — §3 / §5 row 3′. **Owner: T1.4 (duration assembly).** The lexer
  emits an atomic `_` regardless of the chosen form, so the decision defers to where durations are
  actually parsed; only a `tuplet(…)`-block form would later add a lexer keyword.
- **`r` as keyword vs. contextual** — §1 keywords. **Owner: T1.2c/T1.2d.**
- ~~**Block-comment nesting** — §1. **Owner: T1.2b.**~~ → resolved **non-nesting** (C-style, first `*/` closes); now `[CORE]`.
- **Space-before-mark** (`chord.0 .t`) — §6 note. **Owner: T1.4e.**
- ~~**`..` reserved token** — §1; currently no use. **Owner: T1.2e.**~~ → lexes as `DotDot`, kept
  reserved (no parser use yet); revisit if a range/slice construct ever needs it.

Each becomes `[CORE]` (with a snapshot fixture) the moment its construct is implemented and
green.
</content>
</invoke>
