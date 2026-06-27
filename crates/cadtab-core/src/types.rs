//! Minimal static type checker. Not full inference — a small value
//! taxonomy plus arity and value-kind checks, enough to drive crisp in-editor
//! squiggles. Unknown names are *not* reported here (name resolution owns
//! that); this pass assumes names resolve and only checks how they are used.
//!
//! Anything statically unknown (a `def` parameter, an index result) is typed
//! `Unknown`, which absorbs further checks so a single mistake never cascades.

use std::collections::HashMap;

use crate::ast::{
    Block, Event, EventKind, Expr, ExprKind, Item, ItemKind, Program, Repeat, Score, ScoreItemKind,
};
use crate::diagnostics::Diagnostic;
use crate::span::Span;

/// The kind of a value-producing expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Int,
    Duration,
    Position,
    Note,
    Phrase,
    Str,
    /// Statically unknown or already-erroneous; absorbs further checks.
    Unknown,
}

impl Ty {
    /// A noun phrase for diagnostics (carries its article).
    fn label(self) -> &'static str {
        match self {
            Ty::Int => "an integer",
            Ty::Duration => "a duration",
            Ty::Position => "a position",
            Ty::Note => "a note",
            Ty::Phrase => "a phrase",
            Ty::Str => "a string",
            Ty::Unknown => "an unknown value",
        }
    }

    /// Indexable / spreadable: a phrase or chord (or unknown).
    fn is_phrase_like(self) -> bool {
        matches!(self, Ty::Phrase | Ty::Unknown)
    }

    /// A fretted thing a technique can target (or unknown).
    fn is_position_like(self) -> bool {
        matches!(self, Ty::Position | Ty::Note | Ty::Unknown)
    }
}

/// What a function parameter accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    Any,
    Phrase,
    Position,
}

impl Expect {
    fn accepts(self, ty: Ty) -> bool {
        match self {
            Expect::Any => true,
            Expect::Phrase => ty.is_phrase_like(),
            Expect::Position => ty.is_position_like(),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Expect::Any => "any value",
            Expect::Phrase => "a phrase",
            Expect::Position => "a position",
        }
    }
}

/// A callable's signature: per-parameter expectations and a return kind.
#[derive(Debug, Clone)]
struct FnSig {
    params: Vec<Expect>,
    ret: Ty,
}

/// The signature of a builtin function, if `name` names one.
fn builtin_sig(name: &str) -> Option<FnSig> {
    let sig = |params, ret| FnSig { params, ret };
    Some(match name {
        // Connecting techniques annotate a target note.
        "hammer" | "pull" | "slide" => sig(vec![Expect::Position, Expect::Position], Ty::Phrase),
        // Single-note techniques.
        "bend" | "choke" | "ghost" => sig(vec![Expect::Position], Ty::Note),
        "len" => sig(vec![Expect::Phrase], Ty::Int),
        _ => return None,
    })
}

/// The outcome of type checking: kind/arity diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typed {
    pub diagnostics: Vec<Diagnostic>,
}

/// Type-check `program` for arity and value-kind errors.
pub fn check(program: &Program) -> Typed {
    check_with_imports(program, &[])
}

/// Type-check `program`, also knowing the signatures of imported `modules` so
/// calls to imported functions are arity-checked. Entry signatures are collected
/// first, so an entry `def` shadows an imported one of the same name.
pub fn check_with_imports(program: &Program, modules: &[Item]) -> Typed {
    let mut c = Checker {
        values: HashMap::new(),
        funcs: HashMap::new(),
        diagnostics: Vec::new(),
    };
    c.collect_signatures(&program.items);
    c.collect_signatures(modules);
    c.check_program(program);
    Typed {
        diagnostics: c.diagnostics,
    }
}

struct Checker {
    /// `let`-bound names → their value type.
    values: HashMap<String, Ty>,
    /// `def` names → signature (all parameters accept anything, body is a phrase).
    funcs: HashMap<String, FnSig>,
    diagnostics: Vec<Diagnostic>,
}

impl Checker {
    fn diag(&mut self, span: Span, message: String, help: &str) {
        self.diagnostics
            .push(Diagnostic::error(span, message).with_help(help));
    }

    // --- build the global environment -------------------------------------

    fn collect_signatures(&mut self, items: &[Item]) {
        // Pass A: def signatures (arity is structural, order-independent).
        for item in items {
            if let ItemKind::Def(def) = &item.kind {
                self.funcs.entry(def.name.name.clone()).or_insert(FnSig {
                    params: vec![Expect::Any; def.params.len()],
                    ret: Ty::Phrase,
                });
            }
        }
        // Pass B: let types, in source order (a forward reference stays Unknown).
        for item in items {
            if let ItemKind::Let(l) = &item.kind {
                let ty = self.infer(&l.value);
                self.values.entry(l.name.name.clone()).or_insert(ty);
            }
        }
    }

    // --- check bodies ------------------------------------------------------

    fn check_program(&mut self, program: &Program) {
        for item in &program.items {
            match &item.kind {
                ItemKind::Def(def) => self.check_block(&def.body),
                ItemKind::Score(score) => self.check_score(score),
                // `let` RHSs were inferred (and checked) in pass B.
                _ => {}
            }
        }
    }

    fn check_score(&mut self, score: &Score) {
        for item in &score.items {
            match &item.kind {
                ScoreItemKind::Event(ev) => self.check_event(ev),
                ScoreItemKind::Pickup(block) | ScoreItemKind::Measure(block) => {
                    self.check_block(block)
                }
                ScoreItemKind::Loop(l) => self.check_block(&l.body),
                ScoreItemKind::Repeat(r) => self.check_repeat(r),
                ScoreItemKind::Time(_)
                | ScoreItemKind::Default(_)
                | ScoreItemKind::Section(_)
                | ScoreItemKind::Error => {}
            }
        }
    }

    fn check_repeat(&mut self, r: &Repeat) {
        for ev in &r.body {
            self.check_event(ev);
        }
        for ending in &r.endings {
            self.check_block(&ending.body);
        }
    }

    fn check_block(&mut self, block: &Block) {
        for ev in &block.events {
            self.check_event(ev);
        }
    }

    fn check_event(&mut self, ev: &Event) {
        match &ev.kind {
            EventKind::Note(n) => {
                self.infer(&n.head);
            }
            EventKind::Phrase(e) => {
                self.infer(e);
            }
            EventKind::Tie(t) => {
                self.check_event(&t.left);
                self.check_event(&t.right);
            }
            EventKind::Chord(_)
            | EventKind::Rest(_)
            | EventKind::ChordSymbol(_)
            | EventKind::Error => {}
        }
    }

    // --- inference ---------------------------------------------------------

    fn infer(&mut self, e: &Expr) -> Ty {
        match &e.kind {
            ExprKind::Int(_) => Ty::Int,
            ExprKind::Str(_) => Ty::Str,
            ExprKind::Position(_) => Ty::Position,
            // A chord literal is an indexable, spreadable sequence.
            ExprKind::Chord(_) => Ty::Phrase,
            ExprKind::Paren(inner) => self.infer(inner),
            ExprKind::Ident(name) => self.ident_ty(name),
            ExprKind::Index { base, .. } => {
                let bt = self.infer(base);
                if !bt.is_phrase_like() {
                    self.diag(
                        base.span,
                        format!("cannot index into {}", bt.label()),
                        "indexing `.N` works on a phrase or chord",
                    );
                }
                // Element types are not tracked.
                Ty::Unknown
            }
            ExprKind::Spread(inner) => {
                let it = self.infer(inner);
                if !it.is_phrase_like() {
                    self.diag(
                        inner.span,
                        format!("can only spread a phrase, not {}", it.label()),
                        "`...` splats a phrase into positional arguments",
                    );
                }
                Ty::Unknown
            }
            ExprKind::Call { callee, args } => self.infer_call(callee, args, e.span),
            ExprKind::Error => Ty::Unknown,
        }
    }

    fn ident_ty(&self, name: &str) -> Ty {
        // A let-bound value has a known type; a function used as a bare value, a
        // parameter, or an (already-reported) unknown name is permissively Unknown.
        self.values.get(name).copied().unwrap_or(Ty::Unknown)
    }

    fn infer_call(&mut self, callee: &Expr, args: &[Expr], call_span: Span) -> Ty {
        // Infer every argument first (catching nested errors) and note spreads.
        let mut has_spread = false;
        let mut arg_tys = Vec::with_capacity(args.len());
        for a in args {
            if matches!(a.kind, ExprKind::Spread(_)) {
                has_spread = true;
            }
            arg_tys.push(self.infer(a));
        }

        if let ExprKind::Ident(name) = &callee.kind {
            // A user `def` overrides a builtin of the same name (the override
            // mechanism); fall back to the builtin signature otherwise.
            if let Some(sig) = self.funcs.get(name).cloned().or_else(|| builtin_sig(name)) {
                self.check_call_sig(name, &sig, args, &arg_tys, has_spread, call_span);
                return sig.ret;
            }
            if let Some(vt) = self.values.get(name).copied() {
                self.diag(
                    callee.span,
                    format!("{} is not callable", vt.label()),
                    "only `def`s and builtin functions can be called",
                );
                return Ty::Unknown;
            }
            // Parameter or unknown name (resolution reports the latter).
            return Ty::Unknown;
        }

        // A computed callee (e.g. an index result): infer for nested errors.
        self.infer(callee);
        Ty::Unknown
    }

    fn check_call_sig(
        &mut self,
        name: &str,
        sig: &FnSig,
        args: &[Expr],
        arg_tys: &[Ty],
        has_spread: bool,
        call_span: Span,
    ) {
        // A spread makes the positional arity dynamic — nothing to align.
        if has_spread {
            return;
        }
        if args.len() != sig.params.len() {
            let n = sig.params.len();
            self.diag(
                call_span,
                format!(
                    "`{name}` expects {n} argument{}, got {}",
                    if n == 1 { "" } else { "s" },
                    args.len()
                ),
                "check the call against the definition's parameters",
            );
            return;
        }
        for (i, (expect, (arg, ty))) in sig
            .params
            .iter()
            .zip(args.iter().zip(arg_tys.iter().copied()))
            .enumerate()
        {
            if !expect.accepts(ty) {
                self.diag(
                    arg.span,
                    format!(
                        "argument {} to `{name}` should be {}, found {}",
                        i + 1,
                        expect.label(),
                        ty.label()
                    ),
                    "pass a value of the expected kind",
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn diags(src: &str) -> Vec<Diagnostic> {
        let parsed = parse(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "source should parse cleanly: {:?}",
            parsed.diagnostics
        );
        check(&parsed.program).diagnostics
    }

    fn messages(src: &str) -> Vec<String> {
        diags(src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn showcase_type_checks_cleanly() {
        let src = include_str!("../../../examples/showcase.ctab");
        assert_eq!(diags(src), vec![]);
    }

    #[test]
    fn licks_program_type_checks_cleanly() {
        let src = "\
def forward_roll(chord) { chord.0 .t  chord.1 .i  chord.2 .m }
let g_chord = [3:0 2:0 1:0]
score {
  default 1/8
  forward_roll(g_chord)
  loop 3 { forward_roll(g_chord) }
  forward_roll(...g_chord)
  len(g_chord)
}";
        assert_eq!(diags(src), vec![]);
    }

    #[test]
    fn too_few_arguments_reported() {
        assert_eq!(
            messages("def f(a) { a.0 }\nscore { f() }"),
            vec!["`f` expects 1 argument, got 0"]
        );
    }

    #[test]
    fn too_many_arguments_reported() {
        assert_eq!(
            messages("def f(a) { a.0 }\nscore { f(3:0, 2:0) }"),
            vec!["`f` expects 1 argument, got 2"]
        );
    }

    #[test]
    fn spread_suspends_the_arity_check() {
        // Arity is dynamic under a spread, so no arity error despite arity 2.
        let src = "def f(a, b) { a.0 }\nlet g = [3:0 2:0]\nscore { f(...g) }";
        assert_eq!(diags(src), vec![]);
    }

    #[test]
    fn calling_a_value_is_reported() {
        assert_eq!(
            messages("let g = [3:0 2:0]\nscore { g(2:0) }"),
            vec!["a phrase is not callable"]
        );
    }

    #[test]
    fn indexing_a_non_phrase_is_reported() {
        assert_eq!(
            messages("let n = 5\nlet bad = n.0"),
            vec!["cannot index into an integer"]
        );
    }

    #[test]
    fn spreading_a_non_phrase_is_reported() {
        assert_eq!(
            messages("def f(x) { x.0 }\nscore { f(...3:0) }"),
            vec!["can only spread a phrase, not a position"]
        );
    }

    #[test]
    fn len_requires_a_phrase() {
        assert_eq!(
            messages("score { len(3:0) }"),
            vec!["argument 1 to `len` should be a phrase, found a position"]
        );
    }

    #[test]
    fn technique_requires_positions() {
        assert_eq!(
            messages("let g = [3:0 2:0]\nscore { hammer(g, 3:2) }"),
            vec!["argument 1 to `hammer` should be a position, found a phrase"]
        );
    }

    #[test]
    fn technique_arity_is_checked() {
        assert_eq!(
            messages("score { bend(1:7, 2:0) }"),
            vec!["`bend` expects 1 argument, got 2"]
        );
    }

    #[test]
    fn parameters_are_unknown_and_never_false_positive() {
        // `x` is a parameter: indexing, spreading and calling it are all allowed.
        let src = "def f(x) { x.0 }\ndef g(x) { len(x) }\ndef h(x) { hammer(x, x) }";
        assert_eq!(diags(src), vec![]);
    }

    #[test]
    fn type_error_corpus_snapshot() {
        let src = "\
let g = [3:0 2:0 1:0]
let n = 5
def roll(chord) { chord.0 .t }
score {
  roll()
  roll(g, g)
  len(n)
  hammer(g, 3:2)
  n.0
  g(2:0)
  f(...g)
}
def f(a) { a.0 }";
        let parsed = parse(src);
        let typed = check(&parsed.program);
        insta::assert_debug_snapshot!(typed.diagnostics);
    }
}
