//! Evaluation: runtime values, the lexical environment, and lowering AST events
//! into musical-model events. A note without a duration suffix inherits the last
//! one (the sticky default); an explicit suffix updates it for following events.
//! Note positions are validated against the instrument as they are lowered.

use std::collections::HashMap;

use crate::ast::{self, Def, Expr, ExprKind, ItemKind, Mark, MarkKind, Program};
use crate::diagnostics::Diagnostic;
use crate::instrument::Instrument;
use crate::model::{
    Chord, ChordNote, Duration, Event, EventKind, Finger, Note, Phrase, Position, RightHand, Strum,
};
use crate::span::Span;

/// A runtime value. The taxonomy mirrors the static one: integers, durations,
/// positions, single notes, and phrases. A chord literal evaluates to a phrase.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(u32),
    Duration(Duration),
    Position(Position),
    Note(Note),
    Phrase(Phrase),
}

impl Value {
    /// A noun phrase naming the value's kind, for diagnostics.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Value::Int(_) => "an integer",
            Value::Duration(_) => "a duration",
            Value::Position(_) => "a position",
            Value::Note(_) => "a note",
            Value::Phrase(_) => "a phrase",
        }
    }
}

/// A lexical scope chain of value bindings. Lookups walk innermost → outermost,
/// so a binding in a nested scope shadows one outside it. The outermost (global)
/// scope is always present and never popped.
#[derive(Debug, Clone)]
pub struct Env {
    scopes: Vec<HashMap<String, Value>>,
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

impl Env {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Enter a fresh nested scope.
    pub fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Leave the innermost scope. The global scope is kept.
    pub fn pop(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Bind `name` in the innermost scope, shadowing any outer binding.
    pub fn define(&mut self, name: impl Into<String>, value: Value) {
        self.scopes
            .last_mut()
            .expect("env always has a global scope")
            .insert(name.into(), value);
    }

    /// Look up `name`, innermost scope first.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    /// Number of active scopes (always ≥ 1).
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// A fresh environment for a function call: the global scope plus an empty
    /// scope for parameters. Function bodies have lexical scope — they see the
    /// globals and their own parameters, never the caller's local bindings.
    fn call_scope(&self) -> Env {
        Env {
            scopes: vec![self.scopes[0].clone(), HashMap::new()],
        }
    }
}

/// The default note duration before any `default` directive or `_N` suffix has
/// seeded the sticky duration: a quarter note.
const INITIAL_DURATION: Duration = Duration { num: 1, den: 4 };

/// Functions expand into phrases; this bounds how deep that expansion may
/// nest, so a recursive `def` fails with a diagnostic rather than hanging.
const MAX_CALL_DEPTH: usize = 64;

/// Lowers AST events into musical-model events, threading the sticky default
/// duration across the run. `def`s expand into phrases that are spliced at the
/// call site; `let`s bind reusable values. Diagnostics (out-of-range positions,
/// malformed durations, runaway recursion) are collected; lowering is
/// best-effort so a single bad event never drops the rest.
pub struct Evaluator {
    instrument: Instrument,
    sticky: Duration,
    env: Env,
    defs: HashMap<String, Def>,
    call_depth: usize,
    diagnostics: Vec<Diagnostic>,
}

impl Evaluator {
    pub fn new(instrument: Instrument) -> Self {
        Self {
            instrument,
            sticky: INITIAL_DURATION,
            env: Env::new(),
            defs: HashMap::new(),
            call_depth: 0,
            diagnostics: Vec::new(),
        }
    }

    /// Seed the sticky default duration (the `default` directive).
    pub fn set_default(&mut self, dur: Duration) {
        self.sticky = dur;
    }

    /// Consume the evaluator, returning the diagnostics gathered so far.
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Register every `def` and evaluate top-level `let`s into the global scope.
    /// Defs are collected first so a call may precede its definition.
    pub fn load(&mut self, program: &Program) {
        for item in &program.items {
            if let ItemKind::Def(def) = &item.kind {
                self.defs.insert(def.name.name.clone(), def.clone());
            }
        }
        for item in &program.items {
            if let ItemKind::Let(l) = &item.kind
                && let Some(value) = self.eval_expr(&l.value)
            {
                self.env.define(l.name.name.clone(), value);
            }
        }
    }

    /// Lower a run of events, threading the sticky duration across them.
    pub fn eval_events(&mut self, events: &[ast::Event]) -> Phrase {
        let mut phrase = Phrase::new();
        for ev in events {
            self.eval_event_into(ev, &mut phrase);
        }
        phrase
    }

    fn eval_event_into(&mut self, ev: &ast::Event, out: &mut Phrase) {
        match &ev.kind {
            ast::EventKind::Note(note) => {
                if let Some(e) = self.eval_note(note, ev.span) {
                    out.push(e);
                }
            }
            ast::EventKind::Chord(chord) => out.push(self.eval_chord(chord, ev.span)),
            ast::EventKind::Rest(rest) => out.push(self.eval_rest(rest, ev.span)),
            // A bare expression event splices a phrase value at the call site.
            ast::EventKind::Phrase(expr) => {
                if let Some(Value::Phrase(ph)) = self.eval_expr(expr) {
                    out.events.extend(ph.events);
                }
            }
            // Ties (the `~` tie flag) and error nodes are lowered by later passes.
            ast::EventKind::Tie(_) | ast::EventKind::Error => {}
        }
    }

    fn eval_note(&mut self, note: &ast::Note, span: Span) -> Option<Event> {
        // The head must evaluate to a position; computed heads that don't
        // (calls, indexes into note phrases) are lowered by later passes.
        let Some(Value::Position(pos)) = self.eval_expr(&note.head) else {
            return None;
        };
        let dur = self.resolve_duration(note.duration.as_ref());
        let right_hand = note.mark.as_ref().map(lower_mark);
        Some(Event::new(
            EventKind::Note(Note {
                pos,
                dur,
                right_hand,
                technique: None,
                tie: false,
            }),
            span,
        ))
    }

    fn eval_chord(&mut self, chord: &ast::Chord, span: Span) -> Event {
        let dur = self.resolve_duration(chord.duration.as_ref());
        let notes: Vec<ChordNote> = chord
            .notes
            .iter()
            .map(|n| ChordNote {
                pos: self.eval_position(&n.position),
                right_hand: n.mark.as_ref().map(lower_mark),
            })
            .collect();
        Event::new(EventKind::Chord(Chord { dur, notes }), span)
    }

    fn eval_rest(&mut self, rest: &ast::Rest, span: Span) -> Event {
        let dur = self.resolve_duration(rest.duration.as_ref());
        Event::new(EventKind::Rest(dur), span)
    }

    /// Evaluate an expression to a value. Indexing and spread are lowered by a
    /// later pass; an unresolved name yields `None` (name resolution reports it).
    fn eval_expr(&mut self, e: &Expr) -> Option<Value> {
        match &e.kind {
            ExprKind::Int(n) => Some(Value::Int(*n)),
            ExprKind::Position(p) => Some(Value::Position(self.eval_position(p))),
            // A chord literal in value position is a sequence of its members.
            ExprKind::Chord(c) => Some(Value::Phrase(self.chord_to_phrase(c))),
            ExprKind::Paren(inner) => self.eval_expr(inner),
            ExprKind::Ident(name) => self.env.get(name).cloned(),
            ExprKind::Call { callee, args } => self.eval_call(callee, args, e.span),
            // Strings are metadata only; indexing and spread are a later pass.
            ExprKind::Str(_) | ExprKind::Index { .. } | ExprKind::Spread(_) | ExprKind::Error => {
                None
            }
        }
    }

    /// Expand a call to a user `def` into a phrase value. Technique builtins and
    /// spread arguments are handled by later passes.
    fn eval_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Option<Value> {
        let ExprKind::Ident(name) = &callee.kind else {
            return None;
        };
        let def = self.defs.get(name)?.clone();
        if args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_))) {
            return None;
        }
        if self.call_depth >= MAX_CALL_DEPTH {
            self.diagnostics.push(
                Diagnostic::error(span, format!("`{name}` expands too deeply"))
                    .with_help("a function cannot call itself without an exit"),
            );
            return None;
        }
        self.call_depth += 1;
        let phrase = self.expand_def(&def, args);
        self.call_depth -= 1;
        Some(Value::Phrase(phrase))
    }

    /// Bind arguments to parameters in a fresh lexical scope and evaluate the
    /// body to a phrase. Arguments are evaluated in the caller's scope; the body
    /// runs with only the globals plus its parameters.
    fn expand_def(&mut self, def: &Def, args: &[Expr]) -> Phrase {
        let arg_values: Vec<Option<Value>> = args.iter().map(|a| self.eval_expr(a)).collect();
        let call_env = self.env.call_scope();
        let caller = std::mem::replace(&mut self.env, call_env);
        for (param, value) in def.params.iter().zip(arg_values) {
            if let Some(v) = value {
                self.env.define(param.name.clone(), v);
            }
        }
        let phrase = self.eval_events(&def.body.events);
        self.env = caller;
        phrase
    }

    /// Lower a chord literal used as a value into a phrase of its members as
    /// individual notes (the seam later passes index and spread over).
    fn chord_to_phrase(&mut self, chord: &ast::Chord) -> Phrase {
        let dur = match chord.duration.as_ref() {
            Some(d) => self.lower_duration(d).unwrap_or(self.sticky),
            None => self.sticky,
        };
        let mut phrase = Phrase::new();
        for n in &chord.notes {
            let note = Note {
                pos: self.eval_position(&n.position),
                dur,
                right_hand: n.mark.as_ref().map(lower_mark),
                technique: None,
                tie: false,
            };
            phrase.push(Event::new(EventKind::Note(note), n.span));
        }
        phrase
    }

    /// Validate a position against the instrument (string range, fret bound) and
    /// lower it; out-of-range values diagnose but still produce a position.
    fn eval_position(&mut self, pos: &ast::Position) -> Position {
        let (string, fret) = (pos.string.value, pos.fret.value);
        if let Err(diag) = self.instrument.pitch_at(string, fret, pos.span) {
            self.diagnostics.push(diag);
        }
        Position::new(
            u8::try_from(string).unwrap_or(u8::MAX),
            u8::try_from(fret).unwrap_or(u8::MAX),
        )
    }

    /// Resolve a duration: an explicit `_N` updates and returns the sticky
    /// duration; an omitted one inherits the current sticky.
    fn resolve_duration(&mut self, dur: Option<&ast::Duration>) -> Duration {
        if let Some(d) = dur
            && let Some(model) = self.lower_duration(d)
        {
            self.sticky = model;
        }
        self.sticky
    }

    /// Lower a `_N` suffix to a model duration, or diagnose a zero denominator.
    fn lower_duration(&mut self, d: &ast::Duration) -> Option<Duration> {
        if d.denom.value == 0 {
            self.diagnostics.push(
                Diagnostic::error(d.span, "duration denominator must be nonzero")
                    .with_help("a duration is `_N`, where N is 1, 2, 4, 8, …"),
            );
            return None;
        }
        Some(Duration::from_denominator(d.denom.value).dotted(d.dots))
    }
}

fn lower_mark(mark: &Mark) -> RightHand {
    match mark.kind {
        MarkKind::Thumb => RightHand::Finger(Finger::Thumb),
        MarkKind::Index => RightHand::Finger(Finger::Index),
        MarkKind::Middle => RightHand::Finger(Finger::Middle),
        MarkKind::StrumDown => RightHand::Strum(Strum::Down),
        MarkKind::StrumUp => RightHand::Strum(Strum::Up),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Position;

    fn pos(string: u8, fret: u8) -> Value {
        Value::Position(Position::new(string, fret))
    }

    #[test]
    fn kind_labels_name_each_variant() {
        assert_eq!(Value::Int(3).kind_label(), "an integer");
        assert_eq!(pos(3, 0).kind_label(), "a position");
        assert_eq!(
            Value::Phrase(crate::model::Phrase::new()).kind_label(),
            "a phrase"
        );
    }

    #[test]
    fn define_and_get_round_trip() {
        let mut env = Env::new();
        env.define("x", Value::Int(5));
        assert_eq!(env.get("x"), Some(&Value::Int(5)));
        assert_eq!(env.get("missing"), None);
    }

    #[test]
    fn inner_scope_shadows_outer() {
        let mut env = Env::new();
        env.define("g", pos(3, 0));
        env.push();
        env.define("g", pos(1, 7)); // shadows
        assert_eq!(env.get("g"), Some(&pos(1, 7)));
        env.pop();
        // The outer binding is visible again.
        assert_eq!(env.get("g"), Some(&pos(3, 0)));
    }

    #[test]
    fn nested_scope_sees_outer_bindings() {
        let mut env = Env::new();
        env.define("outer", Value::Int(1));
        env.push();
        env.define("inner", Value::Int(2));
        assert_eq!(env.get("outer"), Some(&Value::Int(1)));
        assert_eq!(env.get("inner"), Some(&Value::Int(2)));
        env.pop();
        // The nested binding is gone once its scope ends.
        assert_eq!(env.get("inner"), None);
    }

    #[test]
    fn depth_tracks_scope_nesting_and_global_is_protected() {
        let mut env = Env::new();
        assert_eq!(env.depth(), 1);
        env.push();
        assert_eq!(env.depth(), 2);
        env.pop();
        assert_eq!(env.depth(), 1);
        // Popping past the global scope is a no-op.
        env.pop();
        assert_eq!(env.depth(), 1);
    }

    #[test]
    fn redefining_in_the_same_scope_overwrites() {
        let mut env = Env::new();
        env.define("x", Value::Int(1));
        env.define("x", Value::Int(2));
        assert_eq!(env.get("x"), Some(&Value::Int(2)));
    }
}

#[cfg(test)]
mod event_eval_tests {
    use super::*;
    use crate::ast::{ItemKind, ScoreItemKind};
    use crate::model::{EventKind, Finger, RightHand};
    use crate::parser::parse;

    /// Evaluate the events of a `score { … }` body in source order, applying any
    /// `default` directive and threading the sticky duration across events.
    fn eval_score(src: &str) -> (Phrase, Vec<Diagnostic>) {
        let parsed = parse(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "source should parse cleanly: {:?}",
            parsed.diagnostics
        );
        let mut ev = Evaluator::new(Instrument::builtin("banjo").unwrap());
        ev.load(&parsed.program);
        let mut phrase = Phrase::new();
        for item in &parsed.program.items {
            let ItemKind::Score(score) = &item.kind else {
                continue;
            };
            for si in &score.items {
                match &si.kind {
                    ScoreItemKind::Default(frac) => {
                        ev.set_default(Duration::new(frac.num.value, frac.den.value));
                    }
                    ScoreItemKind::Event(e) => {
                        phrase
                            .events
                            .extend(ev.eval_events(std::slice::from_ref(e)).events);
                    }
                    _ => {}
                }
            }
        }
        (phrase, ev.into_diagnostics())
    }

    fn durs(phrase: &Phrase) -> Vec<Duration> {
        phrase
            .events
            .iter()
            .map(|e| match &e.kind {
                EventKind::Note(n) => n.dur,
                EventKind::Chord(c) => c.dur,
                EventKind::Rest(d) => *d,
            })
            .collect()
    }

    #[test]
    fn note_carries_position_mark_and_default_duration() {
        let (phrase, diags) = eval_score("score { default 1/8\n 3:0.t }");
        assert!(diags.is_empty());
        assert_eq!(phrase.len(), 1);
        let EventKind::Note(n) = &phrase.events[0].kind else {
            panic!("expected a note");
        };
        assert_eq!(n.pos, Position::new(3, 0));
        assert_eq!(n.dur, Duration::new(1, 8));
        assert_eq!(n.right_hand, Some(RightHand::Finger(Finger::Thumb)));
        assert!(!n.tie);
        assert_eq!(n.technique, None);
    }

    #[test]
    fn sticky_duration_inherits_then_updates() {
        // 1/8 seeds; the bare notes inherit it; `_4` updates the sticky for the
        // notes that follow.
        let (phrase, diags) = eval_score("score { default 1/8\n 3:0 2:0 1:0_4 3:2 }");
        assert!(diags.is_empty());
        assert_eq!(
            durs(&phrase),
            vec![
                Duration::new(1, 8),
                Duration::new(1, 8),
                Duration::new(1, 4),
                Duration::new(1, 4),
            ]
        );
    }

    #[test]
    fn unseeded_duration_defaults_to_a_quarter() {
        let (phrase, _) = eval_score("score { 3:0 }");
        assert_eq!(durs(&phrase), vec![Duration::new(1, 4)]);
    }

    #[test]
    fn dotted_duration_is_threaded() {
        let (phrase, _) = eval_score("score { 3:0_4. 2:0 }");
        // A dotted quarter is 3/8; the following bare note inherits it.
        assert_eq!(
            durs(&phrase),
            vec![Duration::new(3, 8), Duration::new(3, 8)]
        );
    }

    #[test]
    fn chord_shares_one_duration_and_per_note_marks() {
        let (phrase, diags) = eval_score("score { default 1/8\n [1:0.m 5:0.t]_4 }");
        assert!(diags.is_empty());
        let EventKind::Chord(c) = &phrase.events[0].kind else {
            panic!("expected a chord");
        };
        assert_eq!(c.dur, Duration::new(1, 4));
        assert_eq!(c.notes.len(), 2);
        assert_eq!(c.notes[0].pos, Position::new(1, 0));
        assert_eq!(
            c.notes[0].right_hand,
            Some(RightHand::Finger(Finger::Middle))
        );
        assert_eq!(c.notes[1].pos, Position::new(5, 0));
        assert_eq!(
            c.notes[1].right_hand,
            Some(RightHand::Finger(Finger::Thumb))
        );
    }

    #[test]
    fn chord_duration_updates_the_sticky() {
        let (phrase, _) = eval_score("score { default 1/8\n [3:0 2:0]_4 1:0 }");
        assert_eq!(
            durs(&phrase),
            vec![Duration::new(1, 4), Duration::new(1, 4)]
        );
    }

    #[test]
    fn rest_carries_duration_and_threads_sticky() {
        let (phrase, diags) = eval_score("score { default 1/8\n r_4 r 3:0 }");
        assert!(diags.is_empty());
        assert!(matches!(phrase.events[0].kind, EventKind::Rest(_)));
        assert_eq!(
            durs(&phrase),
            vec![
                Duration::new(1, 4),
                Duration::new(1, 4),
                Duration::new(1, 4)
            ]
        );
    }

    #[test]
    fn out_of_range_string_diagnoses_but_still_lowers() {
        let (phrase, diags) = eval_score("score { 9:0 }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("out of range"));
        // Best-effort: the note is still emitted.
        assert_eq!(phrase.len(), 1);
    }

    #[test]
    fn unreasonable_fret_diagnoses() {
        let (_, diags) = eval_score("score { 1:99 }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unreasonably high"));
    }

    #[test]
    fn event_lowering_snapshot() {
        let (phrase, _) = eval_score(
            "score {
  default 1/8
  3:0.t  2:0  1:0.m
  3:2_4  3:4
  [1:0.m 5:0.t]_4
  r_8  r
}",
        );
        insta::assert_debug_snapshot!(phrase);
    }

    /// The notes of a phrase as `(position, right-hand, duration)` tuples.
    fn notes(phrase: &Phrase) -> Vec<(Position, Option<RightHand>, Duration)> {
        phrase
            .events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::Note(n) => Some((n.pos, n.right_hand, n.dur)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn call_splices_the_def_body() {
        let (phrase, diags) =
            eval_score("def lick() { 3:0.t 2:0.i 1:0.m }\nscore { default 1/8\n lick() }");
        assert!(diags.is_empty());
        assert_eq!(
            notes(&phrase),
            vec![
                (
                    Position::new(3, 0),
                    Some(RightHand::Finger(Finger::Thumb)),
                    Duration::new(1, 8)
                ),
                (
                    Position::new(2, 0),
                    Some(RightHand::Finger(Finger::Index)),
                    Duration::new(1, 8)
                ),
                (
                    Position::new(1, 0),
                    Some(RightHand::Finger(Finger::Middle)),
                    Duration::new(1, 8)
                ),
            ]
        );
    }

    #[test]
    fn parameters_bind_positionally() {
        // A param used as a note head resolves to the argument's position; the
        // literal's mark and the sticky duration still apply.
        let (phrase, diags) =
            eval_score("def two(a, b) { a.t b.i }\nscore { default 1/8\n two(3:0, 2:0) }");
        assert!(diags.is_empty());
        assert_eq!(
            notes(&phrase),
            vec![
                (
                    Position::new(3, 0),
                    Some(RightHand::Finger(Finger::Thumb)),
                    Duration::new(1, 8)
                ),
                (
                    Position::new(2, 0),
                    Some(RightHand::Finger(Finger::Index)),
                    Duration::new(1, 8)
                ),
            ]
        );
    }

    #[test]
    fn a_let_bound_position_is_a_note_head() {
        let (phrase, diags) = eval_score("let p = 3:0\nscore { p.t }");
        assert!(diags.is_empty());
        assert_eq!(
            notes(&phrase),
            vec![(
                Position::new(3, 0),
                Some(RightHand::Finger(Finger::Thumb)),
                Duration::new(1, 4)
            )]
        );
    }

    #[test]
    fn a_call_may_precede_its_definition() {
        // Defs are collected before any are evaluated, so forward references work.
        let (phrase, diags) = eval_score("score { f() }\ndef f() { 3:0.t }");
        assert!(diags.is_empty());
        assert_eq!(phrase.len(), 1);
    }

    #[test]
    fn calls_are_lexically_scoped() {
        // `inner` cannot see `outer`'s parameter `a`: its `a.t` resolves to
        // nothing, so only `outer`'s own `a.i` produces a note.
        let (phrase, diags) =
            eval_score("def inner() { a.t }\ndef outer(a) { inner() a.i }\nscore { outer(3:0) }");
        assert!(diags.is_empty());
        assert_eq!(
            notes(&phrase),
            vec![(
                Position::new(3, 0),
                Some(RightHand::Finger(Finger::Index)),
                Duration::new(1, 4)
            )]
        );
    }

    #[test]
    fn runaway_recursion_is_bounded() {
        // A self-recursive def terminates with a diagnostic rather than hanging.
        let (phrase, diags) = eval_score("def rec() { rec() }\nscore { rec() }");
        assert!(phrase.is_empty());
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("expands too deeply"))
        );
    }

    #[test]
    fn def_call_lowering_snapshot() {
        let (phrase, _) = eval_score(
            "def roll(a, b, c) { a.t b.i c.m }
let low = 3:0
score {
  default 1/8
  roll(low, 2:0, 1:0)
  roll(5:0, 3:0, 1:0)
}",
        );
        insta::assert_debug_snapshot!(phrase);
    }
}
