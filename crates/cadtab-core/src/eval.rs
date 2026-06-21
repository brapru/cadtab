//! Evaluation: runtime values, the lexical environment, and lowering AST events
//! into musical-model events. A note without a duration suffix inherits the last
//! one (the sticky default); an explicit suffix updates it for following events.
//! Note positions are validated against the instrument as they are lowered.

use std::collections::HashMap;

use crate::ast::{self, Def, Expr, ExprKind, ItemKind, LoopBlock, Mark, MarkKind, Program};
use crate::diagnostics::Diagnostic;
use crate::instrument::Instrument;
use crate::model::{
    Chord, ChordNote, Duration, Event, EventKind, Finger, Measure, Note, Phrase, Position,
    RightHand, Strum, Technique, TimeSig, split_measures,
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

/// A `loop N` unrolls its body inline; this bounds the unroll so a typo'd count
/// cannot freeze the live recompile. Real loops are tiny.
const MAX_LOOP_ITERATIONS: u32 = 1024;

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

    /// Unroll `loop N { body }`: evaluate the body `count` times in sequence,
    /// threading the sticky duration across iterations. A count past the cap is
    /// clamped (with a diagnostic) so a typo cannot blow up the recompile.
    pub fn eval_loop(&mut self, block: &LoopBlock) -> Phrase {
        let mut count = block.count.value;
        if count > MAX_LOOP_ITERATIONS {
            self.diagnostics.push(
                Diagnostic::error(block.count.span, format!("loop count {count} is too large"))
                    .with_help(format!(
                        "a loop unrolls its body inline; keep the count at most {MAX_LOOP_ITERATIONS}"
                    )),
            );
            count = MAX_LOOP_ITERATIONS;
        }
        let mut phrase = Phrase::new();
        for _ in 0..count {
            phrase
                .events
                .extend(self.eval_events(&block.body.events).events);
        }
        phrase
    }

    /// Assemble a score body into measures. Consecutive bare events (and loop
    /// unrolls) form an auto-barred run under the current `time`; an explicit
    /// `measure { }` is taken verbatim as one bar, flushing any run before it.
    /// The meter is stamped on the first measure and after each meter change.
    pub fn eval_score_body(&mut self, items: &[ast::ScoreItem]) -> Vec<Measure> {
        let mut time = TimeSig::new(4, 4);
        let mut pending_meter = Some(time);
        let mut measures = Vec::new();
        let mut run: Vec<Event> = Vec::new();

        for item in items {
            match &item.kind {
                ast::ScoreItemKind::Time(ts) => {
                    flush_run(&mut measures, &mut run, time, &mut pending_meter);
                    if let Some(t) = self.eval_time_sig(ts) {
                        time = t;
                        pending_meter = Some(time);
                    }
                }
                ast::ScoreItemKind::Default(frac) => {
                    self.set_default(Duration::new(frac.num.value, frac.den.value));
                }
                ast::ScoreItemKind::Event(e) => {
                    run.extend(self.eval_events(std::slice::from_ref(e)).events);
                }
                ast::ScoreItemKind::Loop(lb) => {
                    run.extend(self.eval_loop(lb).events);
                }
                ast::ScoreItemKind::Measure(block) => {
                    flush_run(&mut measures, &mut run, time, &mut pending_meter);
                    measures.push(self.block_measure(&block.events, false, &mut pending_meter));
                }
                // A pickup is a partial bar: taken verbatim and flagged so the
                // fill check skips it and the layout renders it offset.
                ast::ScoreItemKind::Pickup(block) => {
                    flush_run(&mut measures, &mut run, time, &mut pending_meter);
                    measures.push(self.block_measure(&block.events, true, &mut pending_meter));
                }
                // Repeats are assembled by a later pass.
                ast::ScoreItemKind::Repeat(_) | ast::ScoreItemKind::Error => {}
            }
        }
        flush_run(&mut measures, &mut run, time, &mut pending_meter);
        measures
    }

    /// Build one verbatim measure from a `measure`/`pickup` block's events,
    /// taking any pending meter stamp.
    fn block_measure(
        &mut self,
        events: &[ast::Event],
        is_pickup: bool,
        pending_meter: &mut Option<TimeSig>,
    ) -> Measure {
        let mut measure = Measure::new(self.eval_events(events).events);
        measure.is_pickup = is_pickup;
        measure.meter = pending_meter.take();
        measure
    }

    /// Convert an AST time signature to the model, validating it. A zero or
    /// oversized value diagnoses and yields `None` (the current meter is kept).
    fn eval_time_sig(&mut self, ts: &ast::TimeSig) -> Option<TimeSig> {
        let num = u8::try_from(ts.num.value).ok().filter(|&n| n > 0);
        let den = u8::try_from(ts.den.value).ok().filter(|&d| d > 0);
        match (num, den) {
            (Some(n), Some(d)) => Some(TimeSig::new(n, d)),
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(ts.span, "invalid time signature")
                        .with_help("write a meter like `4/4` or `6/8`"),
                );
                None
            }
        }
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
            ast::EventKind::Phrase(expr) => match self.eval_expr(expr) {
                Some(Value::Phrase(ph)) => out.events.extend(ph.events),
                // A single-note technique value becomes one note event.
                Some(Value::Note(note)) => out.push(Event::new(EventKind::Note(note), expr.span)),
                // A bare indexed element re-times to a note at the sticky duration.
                Some(Value::Position(pos)) => out.push(Event::new(
                    EventKind::Note(Note {
                        pos,
                        dur: self.sticky,
                        right_hand: None,
                        technique: None,
                        tie: false,
                    }),
                    expr.span,
                )),
                _ => {}
            },
            ast::EventKind::Tie(tie) => self.eval_tie_into(tie, out),
            ast::EventKind::Error => {}
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

    /// Lower a `~` tie: both sides are emitted, and the last note of the left
    /// side is flagged as tying into the right (left-associative chains nest).
    fn eval_tie_into(&mut self, tie: &ast::Tie, out: &mut Phrase) {
        let mut left = Phrase::new();
        self.eval_event_into(&tie.left, &mut left);
        if let Some(EventKind::Note(n)) = left
            .events
            .iter_mut()
            .rev()
            .map(|e| &mut e.kind)
            .find(|k| matches!(k, EventKind::Note(_)))
        {
            n.tie = true;
        }
        out.events.append(&mut left.events);
        self.eval_event_into(&tie.right, out);
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
            ExprKind::Index { base, index } => self.eval_index(base, index.value, e.span),
            // Strings are metadata only; a spread has meaning only inside a call.
            ExprKind::Str(_) | ExprKind::Spread(_) | ExprKind::Error => None,
        }
    }

    /// Index into a phrase value: `base.N` yields the position of its Nth note.
    /// An out-of-range index diagnoses and yields nothing.
    fn eval_index(&mut self, base: &Expr, index: u32, span: Span) -> Option<Value> {
        let Some(Value::Phrase(ph)) = self.eval_expr(base) else {
            return None;
        };
        match ph.events.get(index as usize) {
            Some(ev) => event_to_value(ev),
            None => {
                self.diagnostics.push(
                    Diagnostic::error(
                        span,
                        format!(
                            "index {index} is past the end of a phrase of length {}",
                            ph.events.len()
                        ),
                    )
                    .with_help("phrase indices start at 0"),
                );
                None
            }
        }
    }

    /// Evaluate a call. A user `def` (which overrides a builtin of the same name)
    /// expands into a spliced phrase; otherwise a builtin is tried. Arguments are
    /// evaluated in the caller's scope, expanding any `...` spread.
    fn eval_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Option<Value> {
        let ExprKind::Ident(name) = &callee.kind else {
            return None;
        };
        let arg_values = self.eval_args(args);
        if let Some(def) = self.defs.get(name).cloned() {
            if self.call_depth >= MAX_CALL_DEPTH {
                self.diagnostics.push(
                    Diagnostic::error(span, format!("`{name}` expands too deeply"))
                        .with_help("a function cannot call itself without an exit"),
                );
                return None;
            }
            self.call_depth += 1;
            let phrase = self.expand_def(&def, arg_values);
            self.call_depth -= 1;
            return Some(Value::Phrase(phrase));
        }
        self.eval_builtin(name, &arg_values, span)
    }

    /// Evaluate a call's arguments in the caller's scope, expanding each `...`
    /// spread into the positional values of its phrase.
    fn eval_args(&mut self, args: &[Expr]) -> Vec<Option<Value>> {
        let mut values = Vec::new();
        for a in args {
            if let ExprKind::Spread(inner) = &a.kind {
                match self.eval_expr(inner) {
                    Some(Value::Phrase(ph)) => {
                        values.extend(ph.events.iter().map(event_to_value));
                    }
                    // A non-phrase spread is reported by the type checker.
                    _ => values.push(None),
                }
            } else {
                values.push(self.eval_expr(a));
            }
        }
        values
    }

    /// Evaluate a builtin function: `len` (a general primitive) or a technique
    /// that lowers to a `Technique` annotation. An unknown name yields `None`.
    fn eval_builtin(&mut self, name: &str, args: &[Option<Value>], span: Span) -> Option<Value> {
        match name {
            "len" => match args.first() {
                Some(Some(Value::Phrase(ph))) => Some(Value::Int(ph.events.len() as u32)),
                _ => None,
            },
            // Connecting techniques annotate the target (second) note.
            "hammer" => self.connecting_technique(Technique::HammerOn, args, span),
            "pull" => self.connecting_technique(Technique::PullOff, args, span),
            "slide" => self.connecting_technique(Technique::SlideTo, args, span),
            // Single-note techniques annotate the one note.
            "bend" => self.single_technique(Technique::Bend, args),
            "choke" => self.single_technique(Technique::Choke, args),
            "ghost" => self.single_technique(Technique::Ghost, args),
            _ => None,
        }
    }

    /// `hammer/pull/slide(from, to)`: a phrase of the two notes (at the sticky
    /// duration), with the technique mark on the target note.
    fn connecting_technique(
        &self,
        technique: Technique,
        args: &[Option<Value>],
        span: Span,
    ) -> Option<Value> {
        let from = position_arg(args, 0)?;
        let to = position_arg(args, 1)?;
        let mut phrase = Phrase::new();
        phrase.push(Event::new(
            EventKind::Note(self.technique_note(from, None)),
            span,
        ));
        phrase.push(Event::new(
            EventKind::Note(self.technique_note(to, Some(technique))),
            span,
        ));
        Some(Value::Phrase(phrase))
    }

    /// `bend/choke/ghost(pos)`: one note (at the sticky duration) carrying the
    /// technique mark.
    fn single_technique(&self, technique: Technique, args: &[Option<Value>]) -> Option<Value> {
        let pos = position_arg(args, 0)?;
        Some(Value::Note(self.technique_note(pos, Some(technique))))
    }

    fn technique_note(&self, pos: Position, technique: Option<Technique>) -> Note {
        Note {
            pos,
            dur: self.sticky,
            right_hand: None,
            technique,
            tie: false,
        }
    }

    /// Bind argument values to parameters in a fresh lexical scope and evaluate
    /// the body to a phrase. The body runs with only the globals plus its
    /// parameters; surplus arguments (e.g. from a spread) are ignored.
    fn expand_def(&mut self, def: &Def, arg_values: Vec<Option<Value>>) -> Phrase {
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

/// Auto-bar a run of pending events under `time`, appending the measures and
/// stamping the meter on the first one if a change is pending. Clears the run.
fn flush_run(
    measures: &mut Vec<Measure>,
    run: &mut Vec<Event>,
    time: TimeSig,
    pending_meter: &mut Option<TimeSig>,
) {
    if run.is_empty() {
        return;
    }
    let mut barred = split_measures(std::mem::take(run), time);
    if let Some(first) = barred.first_mut() {
        first.meter = pending_meter.take();
    }
    measures.append(&mut barred);
}

/// The position of a call's `i`th argument, if it is one. Non-position or
/// missing arguments yield `None` (the type checker reports the misuse).
fn position_arg(args: &[Option<Value>], i: usize) -> Option<Position> {
    match args.get(i) {
        Some(Some(Value::Position(p))) => Some(*p),
        _ => None,
    }
}

/// The value an indexed or spread phrase element produces: a note contributes
/// its position; other event kinds have no positional value.
fn event_to_value(ev: &Event) -> Option<Value> {
    match &ev.kind {
        EventKind::Note(n) => Some(Value::Position(n.pos)),
        EventKind::Chord(_) | EventKind::Rest(_) => None,
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
    use crate::model::{EventKind, Finger, Measure, RightHand, Technique, TimeSig};
    use crate::parser::parse;

    /// Assemble every `score { … }` body into measures.
    fn barred(src: &str) -> (Vec<Measure>, Vec<Diagnostic>) {
        let parsed = parse(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "source should parse cleanly: {:?}",
            parsed.diagnostics
        );
        let mut ev = Evaluator::new(Instrument::builtin("banjo").unwrap());
        ev.load(&parsed.program);
        let mut measures = Vec::new();
        for item in &parsed.program.items {
            if let ItemKind::Score(score) = &item.kind {
                measures.extend(ev.eval_score_body(&score.items));
            }
        }
        (measures, ev.into_diagnostics())
    }

    /// Evaluate a score body and flatten its measures back to one event stream —
    /// barring preserves event order, so this is the pre-barring view.
    fn eval_score(src: &str) -> (Phrase, Vec<Diagnostic>) {
        let (measures, diags) = barred(src);
        let events = measures.into_iter().flat_map(|m| m.events).collect();
        (Phrase { events }, diags)
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

    /// The positions of a phrase's notes, in order.
    fn positions(phrase: &Phrase) -> Vec<Position> {
        notes(phrase).into_iter().map(|(p, _, _)| p).collect()
    }

    #[test]
    fn loop_unrolls_its_body_n_times() {
        let (phrase, diags) = eval_score("score { default 1/8\n loop 2 { 3:2 3:4 } }");
        assert!(diags.is_empty());
        assert_eq!(
            positions(&phrase),
            vec![
                Position::new(3, 2),
                Position::new(3, 4),
                Position::new(3, 2),
                Position::new(3, 4),
            ]
        );
    }

    #[test]
    fn loop_zero_unrolls_to_nothing() {
        let (phrase, diags) = eval_score("score { loop 0 { 3:0 2:0 } }");
        assert!(diags.is_empty());
        assert!(phrase.is_empty());
    }

    #[test]
    fn loop_threads_the_sticky_duration_across_iterations() {
        // `_4` in the first iteration sets the sticky; it persists into the next.
        let (phrase, _) = eval_score("score { default 1/8\n loop 2 { 3:2_4 3:4 } }");
        assert_eq!(durs(&phrase), vec![Duration::new(1, 4); 4]);
    }

    #[test]
    fn loop_unrolls_calls_in_its_body() {
        let (phrase, diags) =
            eval_score("def lick() { 3:0.t 2:0.i }\nscore { default 1/8\n loop 2 { lick() } }");
        assert!(diags.is_empty());
        assert_eq!(
            positions(&phrase),
            vec![
                Position::new(3, 0),
                Position::new(2, 0),
                Position::new(3, 0),
                Position::new(2, 0),
            ]
        );
    }

    #[test]
    fn an_oversized_loop_count_is_clamped_with_a_diagnostic() {
        let (phrase, diags) = eval_score("score { loop 100000 { 3:0 } }");
        assert!(diags.iter().any(|d| d.message.contains("too large")));
        assert_eq!(phrase.len(), MAX_LOOP_ITERATIONS as usize);
    }

    #[test]
    fn loop_unroll_snapshot() {
        let (phrase, _) = eval_score("score { default 1/8\n loop 2 { 3:2.t 3:4 } }");
        insta::assert_debug_snapshot!(phrase);
    }

    /// Evaluate the first score phrase-event's expression directly, so a value
    /// that produces no model events (like `len`) can still be inspected.
    fn eval_first_expr(src: &str) -> (Option<Value>, Vec<Diagnostic>) {
        let parsed = parse(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "source should parse cleanly: {:?}",
            parsed.diagnostics
        );
        let mut ev = Evaluator::new(Instrument::builtin("banjo").unwrap());
        ev.load(&parsed.program);
        for item in &parsed.program.items {
            let ItemKind::Score(score) = &item.kind else {
                continue;
            };
            for si in &score.items {
                if let ScoreItemKind::Event(e) = &si.kind
                    && let crate::ast::EventKind::Phrase(expr) = &e.kind
                {
                    let value = ev.eval_expr(expr);
                    return (value, ev.into_diagnostics());
                }
            }
        }
        panic!("no phrase-event expression in score");
    }

    #[test]
    fn indexing_a_chord_yields_its_member_position() {
        // The forward roll re-times a chord's members into a thumb/index/middle
        // sequence — phrase indexing reapplies the right hand and timing.
        let (phrase, diags) = eval_score(
            "def forward_roll(chord) { chord.0 .t  chord.1 .i  chord.2 .m }
let g = [3:0 2:0 1:0]
score { default 1/8\n forward_roll(g) }",
        );
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
    fn a_bare_index_re_times_to_a_note() {
        let (phrase, diags) = eval_score("let g = [3:0 2:2]\nscore { default 1/8\n g.1 }");
        assert!(diags.is_empty());
        assert_eq!(
            notes(&phrase),
            vec![(Position::new(2, 2), None, Duration::new(1, 8))]
        );
    }

    #[test]
    fn len_counts_a_phrase_s_elements() {
        let (value, diags) = eval_first_expr("let g = [3:0 2:0 1:0]\nscore { len(g) }");
        assert!(diags.is_empty());
        assert_eq!(value, Some(Value::Int(3)));
    }

    #[test]
    fn spread_splats_a_phrase_into_positional_arguments() {
        let (phrase, diags) = eval_score(
            "def two(a, b) { a.t b.i }\nlet g = [3:2 1:0]\nscore { default 1/8\n two(...g) }",
        );
        assert!(diags.is_empty());
        assert_eq!(
            notes(&phrase),
            vec![
                (
                    Position::new(3, 2),
                    Some(RightHand::Finger(Finger::Thumb)),
                    Duration::new(1, 8)
                ),
                (
                    Position::new(1, 0),
                    Some(RightHand::Finger(Finger::Index)),
                    Duration::new(1, 8)
                ),
            ]
        );
    }

    #[test]
    fn spread_with_surplus_elements_binds_only_the_parameters() {
        // `one` takes a single parameter; the extra spread elements are dropped.
        let (phrase, diags) =
            eval_score("def one(a) { a.t }\nlet g = [3:0 2:0 1:0]\nscore { one(...g) }");
        assert!(diags.is_empty());
        assert_eq!(positions(&phrase), vec![Position::new(3, 0)]);
    }

    #[test]
    fn an_out_of_range_index_diagnoses() {
        let (phrase, diags) = eval_score("let g = [3:0 2:0]\nscore { g.5 .t }");
        assert!(phrase.is_empty());
        assert!(diags.iter().any(|d| d.message.contains("past the end")));
    }

    #[test]
    fn index_and_spread_snapshot() {
        let (phrase, _) = eval_score(
            "def forward_roll(chord) { chord.0 .t  chord.1 .i  chord.2 .m }
let g = [3:0 2:0 1:0]
score {
  default 1/8
  forward_roll(g)
  loop 2 { forward_roll(g) }
}",
        );
        insta::assert_debug_snapshot!(phrase);
    }

    /// The techniques on a phrase's notes, in order.
    fn techniques(phrase: &Phrase) -> Vec<Option<Technique>> {
        phrase
            .events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::Note(n) => Some(n.technique),
                _ => None,
            })
            .collect()
    }

    /// The tie flags on a phrase's notes, in order.
    fn ties(phrase: &Phrase) -> Vec<bool> {
        phrase
            .events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::Note(n) => Some(n.tie),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn hammer_marks_the_target_note() {
        // Two notes at the sticky duration; the hammer lands on the second.
        let (phrase, diags) = eval_score("score { default 1/8\n hammer(3:0, 3:2) }");
        assert!(diags.is_empty());
        assert_eq!(
            positions(&phrase),
            vec![Position::new(3, 0), Position::new(3, 2)]
        );
        assert_eq!(techniques(&phrase), vec![None, Some(Technique::HammerOn)]);
        assert_eq!(durs(&phrase), vec![Duration::new(1, 8); 2]);
    }

    #[test]
    fn pull_and_slide_mark_the_target_note() {
        let (pull, _) = eval_score("score { pull(1:2, 1:0) }");
        assert_eq!(techniques(&pull), vec![None, Some(Technique::PullOff)]);
        let (slide, _) = eval_score("score { slide(2:5, 2:7) }");
        assert_eq!(techniques(&slide), vec![None, Some(Technique::SlideTo)]);
    }

    #[test]
    fn single_note_techniques_annotate_the_one_note() {
        let (bend, diags) = eval_score("score { default 1/8\n bend(1:7) }");
        assert!(diags.is_empty());
        assert_eq!(positions(&bend), vec![Position::new(1, 7)]);
        assert_eq!(techniques(&bend), vec![Some(Technique::Bend)]);
        assert_eq!(durs(&bend), vec![Duration::new(1, 8)]);

        let (choke, _) = eval_score("score { choke(1:5) }");
        assert_eq!(techniques(&choke), vec![Some(Technique::Choke)]);
        let (ghost, _) = eval_score("score { ghost(3:0) }");
        assert_eq!(techniques(&ghost), vec![Some(Technique::Ghost)]);
    }

    #[test]
    fn a_def_may_override_a_technique_builtin() {
        // A user `def hammer` shadows the builtin (the override mechanism).
        let (phrase, _) = eval_score("def hammer() { 5:0.t }\nscore { hammer() }");
        assert_eq!(positions(&phrase), vec![Position::new(5, 0)]);
        assert_eq!(techniques(&phrase), vec![None]);
    }

    #[test]
    fn tie_flags_the_first_note() {
        let (phrase, diags) = eval_score("score { default 1/8\n 3:2 ~ 3:2 }");
        assert!(diags.is_empty());
        assert_eq!(
            positions(&phrase),
            vec![Position::new(3, 2), Position::new(3, 2)]
        );
        assert_eq!(ties(&phrase), vec![true, false]);
    }

    #[test]
    fn a_tie_chain_flags_every_note_but_the_last() {
        let (phrase, _) = eval_score("score { 3:2 ~ 3:2 ~ 3:2 }");
        assert_eq!(ties(&phrase), vec![true, true, false]);
    }

    #[test]
    fn a_rest_then_a_tie_threads_the_sticky_duration() {
        // The shape of a first ending: a rest, then a tied pair.
        let (phrase, diags) = eval_score("score { default 1/8\n r_8  3:2 ~ 3:2 }");
        assert!(diags.is_empty());
        assert!(matches!(phrase.events[0].kind, EventKind::Rest(_)));
        assert_eq!(ties(&phrase), vec![true, false]);
        assert_eq!(durs(&phrase), vec![Duration::new(1, 8); 3]);
    }

    #[test]
    fn techniques_and_ties_snapshot() {
        let (phrase, _) = eval_score(
            "score {
  default 1/8
  hammer(3:0, 3:2)
  bend(1:7)
  3:2 ~ 3:2
}",
        );
        insta::assert_debug_snapshot!(phrase);
    }

    /// The number of events in each measure, in order.
    fn measure_sizes(measures: &[Measure]) -> Vec<usize> {
        measures.iter().map(|m| m.events.len()).collect()
    }

    #[test]
    fn bare_events_auto_bar_into_measures() {
        // Four quarters fill a 4/4 bar; the meter is stamped on the first measure.
        let (measures, diags) = barred("score { default 1/4\n 3:0 2:0 1:0 5:0 }");
        assert!(diags.is_empty());
        assert_eq!(measure_sizes(&measures), vec![4]);
        assert_eq!(measures[0].meter, Some(TimeSig::new(4, 4)));
    }

    #[test]
    fn an_explicit_measure_is_one_bar_verbatim() {
        // Five quarters in a `measure {}` stay one bar — not re-split by barring.
        let (measures, diags) = barred("score { default 1/4\n measure { 3:0 2:0 1:0 5:0 3:0 } }");
        assert!(diags.is_empty());
        assert_eq!(measure_sizes(&measures), vec![5]);
    }

    #[test]
    fn an_explicit_measure_flushes_the_auto_barred_run_before_it() {
        // A partial run before the brace flushes as its own (under-full) measure.
        let (measures, _) = barred("score { default 1/4\n 3:0 2:0  measure { 1:0 } }");
        assert_eq!(measure_sizes(&measures), vec![2, 1]);
    }

    #[test]
    fn auto_barring_resumes_after_an_explicit_measure() {
        let (measures, _) = barred(
            "score {
  default 1/4
  3:0 2:0 1:0 5:0
  measure { 3:2 }
  1:0 2:0 1:0 5:0
}",
        );
        // Full bar, then the verbatim measure, then another full bar.
        assert_eq!(measure_sizes(&measures), vec![4, 1, 4]);
    }

    #[test]
    fn a_meter_change_is_stamped_on_its_first_measure() {
        let (measures, diags) = barred(
            "score {
  time 4/4
  default 1/4
  3:0 2:0 1:0 5:0
  time 3/4
  3:0 2:0 1:0
}",
        );
        assert!(diags.is_empty());
        assert_eq!(measure_sizes(&measures), vec![4, 3]);
        assert_eq!(measures[0].meter, Some(TimeSig::new(4, 4)));
        assert_eq!(measures[1].meter, Some(TimeSig::new(3, 4)));
    }

    #[test]
    fn an_explicit_first_measure_carries_the_meter() {
        let (measures, _) = barred("score { time 3/4\n measure { 3:0 } }");
        assert_eq!(measures[0].meter, Some(TimeSig::new(3, 4)));
    }

    #[test]
    fn an_invalid_time_signature_diagnoses_and_keeps_the_current_meter() {
        let (measures, diags) = barred("score { time 4/0\n default 1/4\n 3:0 2:0 1:0 5:0 }");
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("invalid time signature"))
        );
        // The bad change is ignored; the default 4/4 stays in effect.
        assert_eq!(measure_sizes(&measures), vec![4]);
        assert_eq!(measures[0].meter, Some(TimeSig::new(4, 4)));
    }

    #[test]
    fn measure_assembly_snapshot() {
        let (measures, _) = barred(
            "score {
  time 4/4
  default 1/4
  3:0 2:0 1:0 5:0
  measure { hammer(3:0, 3:2)  1:0 }
  3:2 3:4 3:0 2:0
}",
        );
        insta::assert_debug_snapshot!(measures);
    }

    /// The pickup flag on each measure, in order.
    fn pickups(measures: &[Measure]) -> Vec<bool> {
        measures.iter().map(|m| m.is_pickup).collect()
    }

    #[test]
    fn pickup_is_a_flagged_partial_bar() {
        let (measures, diags) = barred("score { time 4/4\n default 1/8\n pickup { 2:0 1:0 } }");
        assert!(diags.is_empty());
        assert_eq!(measure_sizes(&measures), vec![2]);
        assert!(measures[0].is_pickup);
    }

    #[test]
    fn a_leading_pickup_carries_the_meter() {
        let (measures, _) = barred("score { time 4/4\n default 1/8\n pickup { 2:0 1:0 } }");
        assert_eq!(measures[0].meter, Some(TimeSig::new(4, 4)));
    }

    #[test]
    fn a_pickup_does_not_offset_the_following_bar_grid() {
        // The pickup's 1/4 is "extra"; the next bar is a fresh, full 4/4 bar.
        let (measures, _) = barred(
            "score {
  time 4/4
  default 1/4
  pickup { 1:0 }
  3:0 2:0 1:0 5:0
}",
        );
        assert_eq!(measure_sizes(&measures), vec![1, 4]);
        assert_eq!(pickups(&measures), vec![true, false]);
    }

    #[test]
    fn a_pickup_is_verbatim_not_resplit() {
        // Five quarters in a pickup stay one bar — the fill check will skip it.
        let (measures, _) = barred("score { default 1/4\n pickup { 3:0 2:0 1:0 5:0 3:0 } }");
        assert_eq!(measure_sizes(&measures), vec![5]);
        assert!(measures[0].is_pickup);
    }

    #[test]
    fn a_pickup_flushes_any_run_before_it() {
        let (measures, _) = barred("score { default 1/4\n 3:0 2:0  pickup { 1:0 } }");
        assert_eq!(measure_sizes(&measures), vec![2, 1]);
        assert_eq!(pickups(&measures), vec![false, true]);
    }
}
