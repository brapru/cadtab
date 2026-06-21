//! Name resolution. Binds every identifier *use* to one of: a `def` parameter,
//! a top-level `def`/`let`, a builtin function, or a name brought in by an
//! `import`; everything else is reported as unknown. Top-level declarations are
//! hoisted, so ordering is free (a call may precede the `def` it names).
//!
//! Scopes are shallow by construction: `def`/`let` live only at the top level
//! (the grammar has no block-local bindings), so the only nested scope is a
//! `def`'s parameter list. Lookup checks parameters, then top-level names, then
//! builtins, then imports.

use std::collections::HashMap;

use crate::ast::{
    Block, Def, Event, EventKind, Expr, ExprKind, ItemKind, Let, Program, Repeat, Score,
    ScoreItemKind,
};
use crate::diagnostics::Diagnostic;
use crate::span::Span;

/// Builtin functions always in scope: techniques (D8) plus phrase utilities.
const BUILTINS: &[&str] = &["hammer", "pull", "slide", "bend", "choke", "ghost", "len"];

/// Names exported by importable modules, keyed by import path. Injected so the
/// pure core stays IO-free: the desktop build populates it from the filesystem,
/// the web build from the embedded stdlib (D38). Empty by default.
#[derive(Debug, Clone, Default)]
pub struct ImportEnv {
    modules: HashMap<String, Vec<String>>,
}

impl ImportEnv {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Register a module path with the names it exports.
    pub fn with_module(
        mut self,
        path: impl Into<String>,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.modules
            .insert(path.into(), names.into_iter().map(Into::into).collect());
        self
    }

    fn module(&self, path: &str) -> Option<&[String]> {
        self.modules.get(path).map(Vec::as_slice)
    }
}

/// The outcome of resolution: diagnostics for unknown names, duplicate
/// definitions, and unresolvable imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub diagnostics: Vec<Diagnostic>,
}

/// Resolve `program` against the embedded builtins only (no importable modules).
pub fn resolve(program: &Program) -> Resolved {
    resolve_with_imports(program, &ImportEnv::empty())
}

/// Resolve `program`, drawing imported names from `imports`.
pub fn resolve_with_imports(program: &Program, imports: &ImportEnv) -> Resolved {
    let mut r = Resolver {
        globals: HashMap::new(),
        imported: HashMap::new(),
        diagnostics: Vec::new(),
    };
    r.collect_top_level(program, imports);
    r.resolve_uses(program);
    Resolved {
        diagnostics: r.diagnostics,
    }
}

struct Resolver {
    /// User top-level `def`/`let` name → its defining span.
    globals: HashMap<String, Span>,
    /// Name brought in by an `import` → the import's span.
    imported: HashMap<String, Span>,
    diagnostics: Vec<Diagnostic>,
}

/// The lexical scope of a single use site: the enclosing `def`'s parameters
/// (empty at the top level).
type Scope<'a> = &'a HashMap<String, Span>;

impl Resolver {
    // --- pass 1: hoist top-level names ------------------------------------

    fn collect_top_level(&mut self, program: &Program, imports: &ImportEnv) {
        for item in &program.items {
            match &item.kind {
                ItemKind::Def(def) => self.declare_global(&def.name.name, def.name.span),
                ItemKind::Let(l) => self.declare_global(&l.name.name, l.name.span),
                ItemKind::Import(path) => {
                    self.resolve_import(path.value.as_str(), item.span, imports)
                }
                _ => {}
            }
        }
    }

    fn declare_global(&mut self, name: &str, span: Span) {
        if let Some(&first) = self.globals.get(name) {
            self.diagnostics.push(
                Diagnostic::error(span, format!("`{name}` is defined more than once")).with_help(
                    format!(
                        "first defined at byte {}; names must be unique",
                        first.start
                    ),
                ),
            );
            return;
        }
        self.globals.insert(name.to_string(), span);
    }

    fn resolve_import(&mut self, path: &str, span: Span, imports: &ImportEnv) {
        match imports.module(path) {
            Some(names) => {
                for name in names {
                    self.imported.entry(name.clone()).or_insert(span);
                }
            }
            None => self.diagnostics.push(
                Diagnostic::error(span, format!("cannot resolve import \"{path}\""))
                    .with_help("imports resolve to local files (desktop) or the standard library"),
            ),
        }
    }

    // --- pass 2: resolve every use ----------------------------------------

    fn resolve_uses(&mut self, program: &Program) {
        for item in &program.items {
            match &item.kind {
                ItemKind::Let(l) => self.resolve_let(l),
                ItemKind::Def(def) => self.resolve_def(def),
                ItemKind::Score(score) => self.resolve_score(score),
                _ => {}
            }
        }
    }

    fn resolve_let(&mut self, l: &Let) {
        let empty = HashMap::new();
        self.resolve_expr(&l.value, &empty);
    }

    fn resolve_def(&mut self, def: &Def) {
        let mut params: HashMap<String, Span> = HashMap::new();
        for p in &def.params {
            if params.contains_key(&p.name) {
                self.diagnostics.push(
                    Diagnostic::error(p.span, format!("duplicate parameter `{}`", p.name))
                        .with_help("each parameter name must be unique"),
                );
            } else {
                params.insert(p.name.clone(), p.span);
            }
        }
        self.resolve_block(&def.body, &params);
    }

    fn resolve_score(&mut self, score: &Score) {
        let empty = HashMap::new();
        for item in &score.items {
            match &item.kind {
                ScoreItemKind::Event(ev) => self.resolve_event(ev, &empty),
                ScoreItemKind::Pickup(block) | ScoreItemKind::Measure(block) => {
                    self.resolve_block(block, &empty)
                }
                ScoreItemKind::Loop(l) => self.resolve_block(&l.body, &empty),
                ScoreItemKind::Repeat(r) => self.resolve_repeat(r, &empty),
                ScoreItemKind::Time(_) | ScoreItemKind::Default(_) | ScoreItemKind::Error => {}
            }
        }
    }

    fn resolve_repeat(&mut self, r: &Repeat, scope: Scope) {
        for ev in &r.body {
            self.resolve_event(ev, scope);
        }
        for ending in &r.endings {
            self.resolve_block(&ending.body, scope);
        }
    }

    fn resolve_block(&mut self, block: &Block, scope: Scope) {
        for ev in &block.events {
            self.resolve_event(ev, scope);
        }
    }

    fn resolve_event(&mut self, ev: &Event, scope: Scope) {
        match &ev.kind {
            EventKind::Note(n) => self.resolve_expr(&n.head, scope),
            EventKind::Phrase(e) => self.resolve_expr(e, scope),
            EventKind::Tie(t) => {
                self.resolve_event(&t.left, scope);
                self.resolve_event(&t.right, scope);
            }
            // Chords and rests are positions/durations only — no names.
            EventKind::Chord(_) | EventKind::Rest(_) | EventKind::Error => {}
        }
    }

    fn resolve_expr(&mut self, e: &Expr, scope: Scope) {
        match &e.kind {
            ExprKind::Ident(name) => self.resolve_use(name, e.span, scope),
            ExprKind::Index { base, .. } => self.resolve_expr(base, scope),
            ExprKind::Call { callee, args } => {
                self.resolve_expr(callee, scope);
                for a in args {
                    self.resolve_expr(a, scope);
                }
            }
            ExprKind::Spread(inner) | ExprKind::Paren(inner) => self.resolve_expr(inner, scope),
            // Int/Str/Position literals and chords carry no names to resolve.
            ExprKind::Int(_)
            | ExprKind::Str(_)
            | ExprKind::Position(_)
            | ExprKind::Chord(_)
            | ExprKind::Error => {}
        }
    }

    fn resolve_use(&mut self, name: &str, span: Span, scope: Scope) {
        let known = scope.contains_key(name)
            || self.globals.contains_key(name)
            || BUILTINS.contains(&name)
            || self.imported.contains_key(name);
        if !known {
            self.diagnostics.push(
                Diagnostic::error(span, format!("unknown name `{name}`"))
                    .with_help("is it a typo, or a missing `def`, `let`, or `import`?"),
            );
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
        resolve(&parsed.program).diagnostics
    }

    fn messages(src: &str) -> Vec<String> {
        diags(src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn cripple_creek_resolves_cleanly() {
        // hammer / bend are builtins; no user names.
        let src = include_str!("../../../examples/cripple_creek.ctab");
        assert_eq!(diags(src), vec![]);
    }

    #[test]
    fn licks_program_resolves_cleanly() {
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
    fn unknown_value_and_callee_are_reported() {
        let msgs = messages("score { foo  bar(3:0) }");
        assert_eq!(msgs, vec!["unknown name `foo`", "unknown name `bar`"]);
    }

    #[test]
    fn unknown_name_in_let_rhs_is_reported() {
        assert_eq!(messages("let x = mystery"), vec!["unknown name `mystery`"]);
    }

    #[test]
    fn definitions_are_hoisted_so_order_is_free() {
        // `f` is used before it is defined.
        let src = "score { f(3:0) }\ndef f(x) { x }";
        assert_eq!(diags(src), vec![]);
    }

    #[test]
    fn parameter_shadows_a_global_let() {
        // Inside `f`, `chord` is the parameter; at the call site it is the let.
        let src = "\
let chord = [3:0 2:0 1:0]
def f(chord) { chord.0 .t }
score { f(chord) }";
        assert_eq!(diags(src), vec![]);
    }

    #[test]
    fn parameter_used_outside_its_def_is_unknown() {
        let src = "def f(x) { x }\nscore { x }";
        assert_eq!(messages(src), vec!["unknown name `x`"]);
    }

    #[test]
    fn duplicate_top_level_definition_is_reported() {
        let msgs = messages("def f() { 3:0 }\ndef f() { 2:0 }");
        assert_eq!(msgs, vec!["`f` is defined more than once"]);
    }

    #[test]
    fn redefining_a_builtin_is_allowed() {
        // Overriding a builtin lick/technique by name is the override mechanism.
        let src = "def hammer(x) { x }\nscore { hammer(3:0) }";
        assert_eq!(diags(src), vec![]);
    }

    #[test]
    fn duplicate_parameter_is_reported() {
        assert_eq!(
            messages("def f(a, a) { a.0 }"),
            vec!["duplicate parameter `a`"]
        );
    }

    #[test]
    fn unresolved_import_is_reported() {
        let msgs = messages("import \"rolls.ctab\"");
        assert_eq!(msgs, vec!["cannot resolve import \"rolls.ctab\""]);
    }

    #[test]
    fn imported_names_resolve_when_the_module_is_provided() {
        let src = "import \"rolls.ctab\"\nscore { my_roll(3:0) }";
        let parsed = parse(src);
        let imports = ImportEnv::empty().with_module("rolls.ctab", ["my_roll"]);
        let resolved = resolve_with_imports(&parsed.program, &imports);
        assert_eq!(resolved.diagnostics, vec![]);
    }

    #[test]
    fn missing_module_flags_both_import_and_use() {
        let msgs = messages("import \"rolls.ctab\"\nscore { my_roll(3:0) }");
        assert_eq!(
            msgs,
            vec![
                "cannot resolve import \"rolls.ctab\"",
                "unknown name `my_roll`"
            ]
        );
    }

    #[test]
    fn diagnostics_corpus_snapshot() {
        let src = "\
let g = [3:0 2:0 1:0]
def roll(chord) { chord.0 .t  mystery }
def roll(chord) { chord.1 .i }
score {
  roll(g)
  nope(2:0)
  len(g)
}";
        let parsed = parse(src);
        let resolved = resolve(&parsed.program);
        insta::assert_debug_snapshot!(resolved.diagnostics);
    }
}
