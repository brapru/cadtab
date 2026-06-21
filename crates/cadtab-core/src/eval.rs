//! Evaluation: runtime values, the lexical environment, and lowering AST events
//! into musical-model events. A note without a duration suffix inherits the last
//! one (the sticky default); an explicit suffix updates it for following events.
//! Note positions are validated against the instrument as they are lowered.

use std::collections::HashMap;

use crate::ast::{self, ExprKind, Mark, MarkKind};
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
}

/// The default note duration before any `default` directive or `_N` suffix has
/// seeded the sticky duration: a quarter note.
const INITIAL_DURATION: Duration = Duration { num: 1, den: 4 };

/// Lowers AST events into musical-model events, threading the sticky default
/// duration across the run. Diagnostics (out-of-range positions, malformed
/// durations) are collected; lowering is best-effort so a single bad event
/// never drops the rest.
pub struct Evaluator {
    instrument: Instrument,
    sticky: Duration,
    diagnostics: Vec<Diagnostic>,
}

impl Evaluator {
    pub fn new(instrument: Instrument) -> Self {
        Self {
            instrument,
            sticky: INITIAL_DURATION,
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

    /// Lower a run of events, threading the sticky duration across them.
    pub fn eval_events(&mut self, events: &[ast::Event]) -> Phrase {
        let mut phrase = Phrase::new();
        for ev in events {
            if let Some(out) = self.eval_event(ev) {
                phrase.push(out);
            }
        }
        phrase
    }

    fn eval_event(&mut self, ev: &ast::Event) -> Option<Event> {
        match &ev.kind {
            ast::EventKind::Note(note) => self.eval_note(note, ev.span),
            ast::EventKind::Chord(chord) => Some(self.eval_chord(chord, ev.span)),
            ast::EventKind::Rest(rest) => Some(self.eval_rest(rest, ev.span)),
            // Phrase splices, ties, and error nodes are lowered by later passes.
            ast::EventKind::Phrase(_) | ast::EventKind::Tie(_) | ast::EventKind::Error => None,
        }
    }

    fn eval_note(&mut self, note: &ast::Note, span: Span) -> Option<Event> {
        // Only position-literal heads are notes here; computed heads (indexing,
        // calls) are lowered by later passes.
        let ExprKind::Position(p) = &note.head.kind else {
            return None;
        };
        let pos = self.eval_position(p);
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
        if let Some(d) = dur {
            if d.denom.value == 0 {
                self.diagnostics.push(
                    Diagnostic::error(d.span, "duration denominator must be nonzero")
                        .with_help("a duration is `_N`, where N is 1, 2, 4, 8, …"),
                );
                return self.sticky;
            }
            self.sticky = Duration::from_denominator(d.denom.value).dotted(d.dots);
        }
        self.sticky
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
}
