//! The canonical `.ctab` pretty-printer (D47): a pure `format(source) -> String`
//! that re-emits a document in one house style. It is **error-safe** — a source
//! with parse errors is returned untouched (the resilient parser would otherwise
//! drop the broken span), **idempotent** (`format(format(x)) == format(x)`), and
//! **comment-preserving**.
//!
//! The emitter walks the AST (so it knows structure — e.g. where a duration ends,
//! which a flat token re-spacer can't tell a `_8.` augmentation dot from an index
//! `.0`) but threads a *source cursor* alongside it: between siblings it consults
//! the original text for newlines, so author line-grouping (a measure's worth of
//! notes on one line) and blank-line separations survive, and comment trivia —
//! which the parser drops — is re-inserted by span. Atom spacing is fixed by the
//! grammar; only line breaks and blanks follow the source.

use crate::ast::{
    Block, Chord, ChordNote, CustomTuning, Def, Duration, Ending, Event, EventKind, Expr, ExprKind,
    Item, ItemKind, Let, Mark, MarkKind, Note, Position, Program, Rest, Score, ScoreItem,
    ScoreItemKind, StringLit, TuningRef,
};
use crate::lexer::lex;
use crate::parser::parse;
use crate::span::Span;
use crate::token::TokenKind;

/// Pretty-print `source` into the canonical layout. A document with parse errors
/// is returned unchanged (never half-formatted); a clean parse is re-emitted.
pub fn format(source: &str) -> String {
    let parsed = parse(source);
    if !parsed.diagnostics.is_empty() {
        return source.to_string();
    }
    let comments: Vec<Span> = lex(source)
        .tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Comment)
        .map(|t| t.span)
        .collect();
    let mut f = Formatter::new(source, comments);
    f.program(&parsed.program);
    f.finish()
}

struct Formatter<'a> {
    src: &'a str,
    out: String,
    depth: usize,
    /// Comment spans in source order, and the cursor into them.
    comments: Vec<Span>,
    ci: usize,
    /// The source byte offset just past everything emitted so far. Gaps to the
    /// next piece are measured from here.
    cursor: u32,
}

impl<'a> Formatter<'a> {
    fn new(src: &'a str, comments: Vec<Span>) -> Self {
        Self {
            src,
            out: String::new(),
            depth: 0,
            comments,
            ci: 0,
            cursor: 0,
        }
    }

    fn finish(mut self) -> String {
        // Flush any trailing comments (each with its own leading separator), but
        // emit no separator past the final token, then guarantee one trailing
        // newline. A document never ends with stray whitespace.
        let src = self.src;
        let end = src.len() as u32;
        while self.ci < self.comments.len() && self.comments[self.ci].start < end {
            let c = self.comments[self.ci];
            self.sep(self.cursor, c.start, false);
            let text = src[c.start as usize..c.end as usize].trim_end();
            self.out.push_str(text);
            self.cursor = c.end;
            self.ci += 1;
        }
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.out
    }

    // --- low-level emission -------------------------------------------------

    fn push_indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
    }

    /// Emit a single space for a same-source-line gap, or a line break
    /// (preserving one blank line) when the source had a newline — or when
    /// `force_break` and the pieces were on one line. Suppressed at the very
    /// start of output so the document has no leading whitespace.
    fn sep(&mut self, from: u32, to: u32, force_break: bool) {
        if self.out.is_empty() {
            return;
        }
        let src = self.src;
        let newlines = if to <= from {
            0
        } else {
            src[from as usize..to as usize]
                .bytes()
                .filter(|&b| b == b'\n')
                .count()
        };
        if newlines == 0 && !force_break {
            self.out.push(' ');
        } else {
            self.out.push('\n');
            if newlines >= 2 {
                self.out.push('\n');
            }
            self.push_indent();
        }
    }

    /// Emit every comment that begins before `pos`, then the separator up to
    /// `pos`. A comment keeps its place: trailing when it shared a line with the
    /// previous piece, on its own line otherwise. Leaves the cursor at `pos`.
    fn advance_to(&mut self, pos: u32, force_break: bool) {
        let src = self.src;
        while self.ci < self.comments.len() && self.comments[self.ci].start < pos {
            let c = self.comments[self.ci];
            self.sep(self.cursor, c.start, false);
            let text = src[c.start as usize..c.end as usize].trim_end();
            self.out.push_str(text);
            self.cursor = c.end;
            self.ci += 1;
        }
        self.sep(self.cursor, pos, force_break);
    }

    /// Whether a comment begins strictly inside `(lo, hi)` — used to keep an
    /// otherwise-empty block open so its lone comment survives.
    fn comment_in(&self, lo: u32, hi: u32) -> bool {
        self.comments.iter().any(|c| c.start > lo && c.start < hi)
    }

    /// The first `{` at or after `from`, skipping any inside a comment. Used for
    /// the `score`/`repeat` blocks, whose brace positions the AST doesn't carry.
    fn lbrace_after(&self, from: u32) -> u32 {
        let bytes = self.src.as_bytes();
        let mut i = from as usize;
        while i < bytes.len() {
            if let Some(c) = self
                .comments
                .iter()
                .find(|c| (c.start as usize) <= i && i < c.end as usize)
            {
                i = c.end as usize;
                continue;
            }
            if bytes[i] == b'{' {
                return i as u32;
            }
            i += 1;
        }
        from
    }

    // --- top level ----------------------------------------------------------

    fn program(&mut self, program: &Program) {
        for (i, item) in program.items.iter().enumerate() {
            // Top-level items are always one per line; blank lines between them
            // (collapsed to one) are preserved.
            self.advance_to(item.span.start, i != 0);
            self.item(item);
            self.cursor = item.span.end;
        }
    }

    fn item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Title(s) => self.directive_str("title", s),
            ItemKind::Composer(s) => self.directive_str("composer", s),
            ItemKind::Capo(s) => self.directive_str("capo", s),
            ItemKind::Import(s) => self.directive_str("import", s),
            ItemKind::Tempo(n) => {
                self.out.push_str("tempo ");
                self.out.push_str(&n.value.to_string());
            }
            ItemKind::Instrument(id) => {
                self.out.push_str("instrument ");
                self.out.push_str(&id.name);
            }
            ItemKind::BarNumbers(id) => {
                self.out.push_str("barnumbers ");
                self.out.push_str(&id.name);
            }
            ItemKind::Tuning(t) => self.tuning(t),
            ItemKind::Def(d) => self.def(item, d),
            ItemKind::Let(l) => self.let_item(item, l),
            ItemKind::Score(s) => self.score(item, s),
            ItemKind::Error => self.push_source(item.span),
        }
    }

    fn directive_str(&mut self, keyword: &str, s: &StringLit) {
        self.out.push_str(keyword);
        self.out.push(' ');
        self.push_str_lit(s);
    }

    fn tuning(&mut self, t: &TuningRef) {
        self.out.push_str("tuning ");
        match t {
            TuningRef::Named(id) => self.out.push_str(&id.name),
            TuningRef::Custom(ct) => self.custom_tuning(ct),
        }
    }

    fn custom_tuning(&mut self, ct: &CustomTuning) {
        if let Some(name) = &ct.name {
            self.push_str_lit(name);
            self.out.push(' ');
        }
        self.out.push('{');
        for s in &ct.strings {
            self.out.push(' ');
            self.out.push_str(&s.name);
        }
        self.out.push_str(" }");
    }

    fn def(&mut self, item: &Item, d: &Def) {
        self.out.push_str("def ");
        self.out.push_str(&d.name.name);
        self.out.push('(');
        for (i, p) in d.params.iter().enumerate() {
            if i != 0 {
                self.out.push_str(", ");
            }
            self.out.push_str(&p.name);
        }
        self.out.push(')');
        self.block(&d.body);
        self.cursor = item.span.end;
    }

    fn let_item(&mut self, item: &Item, l: &Let) {
        self.out.push_str("let ");
        self.out.push_str(&l.name.name);
        self.out.push_str(" = ");
        self.expr(&l.value);
        self.cursor = item.span.end;
    }

    // --- score --------------------------------------------------------------

    fn score(&mut self, item: &Item, score: &Score) {
        self.out.push_str("score");
        self.cursor = item.span.start + "score".len() as u32;
        let lb = self.lbrace_after(item.span.start);
        self.advance_to(lb, false);
        self.out.push('{');
        self.cursor = lb + 1;

        if score.items.is_empty() && !self.comment_in(lb, item.span.end) {
            self.out.push('}');
            self.cursor = item.span.end;
            return;
        }

        self.depth += 1;
        let mut prev_event = false;
        for (i, si) in score.items.iter().enumerate() {
            let is_event = matches!(si.kind, ScoreItemKind::Event(_));
            // Bare events keep the author's line-grouping (a measure per line);
            // structural items always start a fresh line.
            let force = if i == 0 {
                !is_event
            } else {
                !(is_event && prev_event)
            };
            self.advance_to(si.span.start, force);
            self.score_item(si);
            self.cursor = si.span.end;
            prev_event = is_event;
        }
        self.depth -= 1;
        self.advance_to(item.span.end - 1, false);
        self.out.push('}');
        self.cursor = item.span.end;
    }

    fn score_item(&mut self, si: &ScoreItem) {
        match &si.kind {
            ScoreItemKind::Time(t) => {
                self.out.push_str("time ");
                self.out.push_str(&t.num.value.to_string());
                self.out.push('/');
                self.out.push_str(&t.den.value.to_string());
            }
            ScoreItemKind::Default(fr) => {
                self.out.push_str("default ");
                self.out.push_str(&fr.num.value.to_string());
                self.out.push('/');
                self.out.push_str(&fr.den.value.to_string());
            }
            ScoreItemKind::Section(s) => {
                self.out.push_str("section ");
                self.push_str_lit(s);
            }
            ScoreItemKind::Pickup(b) => {
                self.out.push_str("pickup");
                self.block(b);
            }
            ScoreItemKind::Measure(b) => {
                self.out.push_str("measure");
                self.block(b);
            }
            ScoreItemKind::Loop(lb) => {
                self.out.push_str("loop ");
                self.out.push_str(&lb.count.value.to_string());
                self.block(&lb.body);
            }
            ScoreItemKind::Repeat(r) => self.repeat(si.span, &r.body, &r.endings),
            ScoreItemKind::Event(ev) => self.event(ev),
            ScoreItemKind::Error => self.push_source(si.span),
        }
    }

    /// `repeat { body… ending(n){…}… }`. Body events keep their line-grouping;
    /// each volta starts a fresh line. The brace span isn't in the AST, so the
    /// opening `{` is found in source and the closing `}` is `span.end - 1`.
    fn repeat(&mut self, span: Span, body: &[Event], endings: &[Ending]) {
        self.out.push_str("repeat");
        self.cursor = span.start + "repeat".len() as u32;
        let lb = self.lbrace_after(span.start);
        self.advance_to(lb, false);
        self.out.push('{');
        self.cursor = lb + 1;

        self.depth += 1;
        for ev in body {
            // Body events keep the author's line-grouping, like any block body.
            self.advance_to(ev.span.start, false);
            self.event(ev);
            self.cursor = ev.span.end;
        }
        for end in endings {
            // A volta always starts its own line (even right after body events).
            self.advance_to(end.span.start, true);
            self.ending(end);
            self.cursor = end.span.end;
        }
        self.depth -= 1;
        self.advance_to(span.end - 1, false);
        self.out.push('}');
        self.cursor = span.end;
    }

    fn ending(&mut self, end: &Ending) {
        self.out.push_str("ending(");
        self.out.push_str(&end.number.value.to_string());
        self.out.push(')');
        self.block(&end.body);
    }

    // --- blocks & events ----------------------------------------------------

    /// A braced event body (`def`/`pickup`/`measure`/`loop`/`ending` bodies). The
    /// opening `{` rides on the header line; events keep their source line-
    /// grouping. An empty body with no comment collapses to `{}`.
    fn block(&mut self, b: &Block) {
        self.out.push_str(" {");
        self.cursor = b.span.start + 1; // span starts at `{`
        if b.events.is_empty() && !self.comment_in(b.span.start, b.span.end) {
            self.out.push('}');
            self.cursor = b.span.end;
            return;
        }
        self.depth += 1;
        for ev in &b.events {
            self.advance_to(ev.span.start, false);
            self.event(ev);
            self.cursor = ev.span.end;
        }
        self.depth -= 1;
        self.advance_to(b.span.end - 1, false);
        self.out.push('}');
        self.cursor = b.span.end;
    }

    fn event(&mut self, ev: &Event) {
        match &ev.kind {
            EventKind::Note(n) => self.note(n),
            EventKind::Chord(c) => self.chord(c),
            EventKind::Rest(r) => self.rest(r),
            EventKind::Phrase(e) => self.expr(e),
            EventKind::Tie(t) => {
                self.event(&t.left);
                self.out.push_str(" ~ ");
                self.event(&t.right);
            }
            EventKind::ChordSymbol(s) => {
                self.out.push_str("chord ");
                self.push_str_lit(s);
            }
            EventKind::Error => self.push_source(ev.span),
        }
    }

    fn note(&mut self, n: &Note) {
        self.expr(&n.head);
        if let Some(m) = &n.mark {
            self.push_mark(m);
        }
        if let Some(d) = &n.duration {
            self.push_duration(d);
        }
    }

    fn rest(&mut self, r: &Rest) {
        self.out.push('r');
        if let Some(d) = &r.duration {
            self.push_duration(d);
        }
    }

    fn chord(&mut self, c: &Chord) {
        self.out.push('[');
        for (i, note) in c.notes.iter().enumerate() {
            if i != 0 {
                self.out.push(' ');
            }
            self.chord_note(note);
        }
        self.out.push(']');
        if let Some(d) = &c.duration {
            self.push_duration(d);
        }
    }

    fn chord_note(&mut self, n: &ChordNote) {
        self.push_position(&n.position);
        if let Some(m) = &n.mark {
            self.push_mark(m);
        }
    }

    // --- expressions --------------------------------------------------------

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Int(n) => self.out.push_str(&n.to_string()),
            ExprKind::Str(s) => {
                self.out.push('"');
                self.out.push_str(s);
                self.out.push('"');
            }
            ExprKind::Ident(s) => self.out.push_str(s),
            ExprKind::Position(p) => self.push_position(p),
            ExprKind::Chord(c) => self.chord(c),
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.out.push('.');
                self.out.push_str(&index.value.to_string());
            }
            ExprKind::Call { callee, args } => {
                self.expr(callee);
                self.out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i != 0 {
                        self.out.push_str(", ");
                    }
                    self.expr(a);
                }
                self.out.push(')');
            }
            ExprKind::Spread(inner) => {
                self.out.push_str("...");
                self.expr(inner);
            }
            ExprKind::Paren(inner) => {
                self.out.push('(');
                self.expr(inner);
                self.out.push(')');
            }
            ExprKind::Error => self.push_source(e.span),
        }
    }

    // --- atoms --------------------------------------------------------------

    fn push_position(&mut self, p: &Position) {
        self.out.push_str(&p.string.value.to_string());
        self.out.push(':');
        self.out.push_str(&p.fret.value.to_string());
    }

    fn push_mark(&mut self, m: &Mark) {
        self.out.push_str(match m.kind {
            MarkKind::Thumb => ".t",
            MarkKind::Index => ".i",
            MarkKind::Middle => ".m",
            MarkKind::StrumDown => ".d",
            MarkKind::StrumUp => ".u",
        });
    }

    fn push_duration(&mut self, d: &Duration) {
        self.out.push('_');
        self.out.push_str(&d.denom.value.to_string());
        for _ in 0..d.dots {
            self.out.push('.');
        }
    }

    fn push_str_lit(&mut self, s: &StringLit) {
        self.out.push('"');
        self.out.push_str(&s.value);
        self.out.push('"');
    }

    /// Last-resort fallback for an `Error` node (unreachable on a clean parse,
    /// which `format` requires): echo the original source span verbatim.
    fn push_source(&mut self, span: Span) {
        let src = self.src;
        self.out
            .push_str(&src[span.start as usize..span.end as usize]);
    }
}

#[cfg(test)]
mod tests {
    use super::format;
    use crate::compile;
    use crate::layout::LayoutConfig;

    const CFG: LayoutConfig = LayoutConfig { width: 800.0 };

    /// `format` over a clean parse must be a fixed point of itself.
    fn assert_idempotent(src: &str) {
        let once = format(src);
        let twice = format(&once);
        assert_eq!(
            once, twice,
            "format is not idempotent\n--- once ---\n{once}"
        );
    }

    /// Formatting must not change what a document *means*. Source spans ride
    /// through the render tree (for click-to-source), so they legitimately shift
    /// when byte offsets change; everything else — the notes, positions, labels —
    /// must be identical. Compare the trees with every `span` stripped.
    fn strip_spans(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(map) => {
                map.remove("span");
                for child in map.values_mut() {
                    strip_spans(child);
                }
            }
            serde_json::Value::Array(items) => {
                for child in items.iter_mut() {
                    strip_spans(child);
                }
            }
            _ => {}
        }
    }

    fn assert_semantics_preserved(src: &str) {
        let before = compile(src, CFG);
        let after = compile(&format(src), CFG);
        let mut a = serde_json::to_value(&before.render_tree).unwrap();
        let mut b = serde_json::to_value(&after.render_tree).unwrap();
        strip_spans(&mut a);
        strip_spans(&mut b);
        assert_eq!(
            a, b,
            "render tree changed after formatting (ignoring spans)"
        );
        // No new errors introduced (clean docs stay clean).
        let errs = |r: &crate::CompileResult| {
            r.diagnostics
                .iter()
                .filter(|d| d.severity == crate::diagnostics::Severity::Error)
                .count()
        };
        assert_eq!(errs(&before), errs(&after), "error count changed");
    }

    const SHOWCASE: &str = include_str!("../../../examples/showcase.ctab");
    const CRIPPLE_CREEK: &str = include_str!("../../../examples/cripple-creek.ctab");
    const LICKS: &str = include_str!("../../../examples/licks.ctab");
    const BANJO_TPL: &str = include_str!("../../../examples/templates/banjo.ctab");
    const BLANK_TPL: &str = include_str!("../../../examples/templates/blank.ctab");

    #[test]
    fn bundled_examples_are_idempotent_and_semantics_preserving() {
        for src in [SHOWCASE, CRIPPLE_CREEK, LICKS, BANJO_TPL, BLANK_TPL] {
            assert_idempotent(src);
            assert_semantics_preserved(src);
        }
    }

    #[test]
    fn formatting_is_stable_after_one_pass() {
        // Formatting an already-canonical document is a no-op (the canonical form
        // is a fixed point), demonstrated on the showcase's own formatted text.
        let canonical = format(SHOWCASE);
        assert_eq!(format(&canonical), canonical);
    }

    #[test]
    fn showcase_snapshot() {
        insta::assert_snapshot!(format(SHOWCASE));
    }

    #[test]
    fn collapses_runs_and_normalizes_indentation() {
        let messy = "title   \"X\"\n\n\n\ninstrument banjo\nscore {\n\t\t3:0    2:0\n      1:0\n}";
        let out = format(messy);
        assert_eq!(
            out,
            "title \"X\"\n\ninstrument banjo\nscore {\n  3:0 2:0\n  1:0\n}\n"
        );
        assert_idempotent(messy);
    }

    #[test]
    fn preserves_event_line_grouping() {
        // The author's per-line note grouping (a measure per line) is musically
        // meaningful, so it survives even as spacing is normalized.
        let src = "score {\n  3:0 2:0 1:0 5:0\n  3:2 3:4 2:0 1:0\n}";
        assert_eq!(format(src), format(&format(src)));
        let out = format(src);
        assert!(out.contains("3:0 2:0 1:0 5:0\n"));
        assert!(out.contains("3:2 3:4 2:0 1:0\n"));
    }

    #[test]
    fn a_dotted_duration_stays_separated_from_the_next_event() {
        // The augmentation dot of `_8.` must not fuse with the next note — the
        // case a flat token re-spacer gets wrong.
        let out = format("score {\n  r_8. 3:2\n}");
        assert!(out.contains("r_8. 3:2"), "got: {out}");
        assert!(!out.contains("r_8.3:2"), "dotted duration fused: {out}");
    }

    #[test]
    fn normalizes_atom_spacing() {
        let out = format("score {\n  [1:0.m  5:0.t]_4   r_8\n  3:2~3:2\n  hammer( 3:0 ,3:2 )\n}");
        assert!(out.contains("[1:0.m 5:0.t]_4 r_8"), "got: {out}");
        assert!(out.contains("3:2 ~ 3:2"), "got: {out}");
        assert!(out.contains("hammer(3:0, 3:2)"), "got: {out}");
        assert_idempotent("score {\n  [1:0.m  5:0.t]_4   r_8\n  3:2~3:2\n}");
    }

    #[test]
    fn leading_and_trailing_comments_survive() {
        let src = "// header\ntitle \"X\"  // about the title\nscore {\n  // a note\n  3:0\n}";
        let out = format(src);
        assert!(out.contains("// header\n"));
        assert!(out.contains("title \"X\" // about the title"));
        assert!(out.contains("  // a note\n"));
        assert!(out.contains("  3:0\n"));
        assert_idempotent(src);
    }

    #[test]
    fn a_comment_keeps_an_otherwise_empty_block_open() {
        let out = format("score {\n  // todo\n}");
        assert!(out.contains("// todo"), "comment dropped: {out}");
    }

    #[test]
    fn an_empty_block_collapses() {
        assert_eq!(
            format("def f() {\n}\nscore { 3:0 }"),
            "def f() {}\nscore { 3:0 }\n"
        );
    }

    #[test]
    fn custom_tuning_round_trips() {
        let src = "tuning \"Drop D\" { D4 B3 G3 D3 A2 D2 }\nscore { 1:0 }";
        let out = format(src);
        assert!(
            out.contains("tuning \"Drop D\" { D4 B3 G3 D3 A2 D2 }"),
            "got: {out}"
        );
        assert_idempotent(src);
    }

    #[test]
    fn a_document_with_parse_errors_is_returned_untouched() {
        // The resilient parser would drop the broken span, so a half-formatted
        // result could silently delete code; instead the source is returned as-is.
        let broken = "score {  3:0  ";
        assert_eq!(format(broken), broken);
    }

    #[test]
    fn empty_and_comment_only_inputs() {
        assert_eq!(format(""), "");
        assert_eq!(format("   \n\n"), "");
        assert_eq!(format("// lonely"), "// lonely\n");
    }

    #[test]
    fn formats_every_directive_and_score_construct() {
        // One document touching the remaining node kinds — directives, the score
        // structural items, spread/paren/index/call exprs — must round-trip to a
        // stable canonical form.
        let src = concat!(
            "composer \"C\"\n",
            "capo \"2\"\n",
            "import \"licks.ctab\"\n",
            "barnumbers off\n",
            "let riff = [3:0 2:0]\n",
            "def echo(c) { ...c }\n",
            "score {\n",
            "  pickup { 5:0 }\n",
            "  section \"A\"\n",
            "  loop 2 { 3:2 }\n",
            "  measure { (3:0) echo(riff) c.0 }\n",
            "}\n",
        );
        let out = format(src);
        assert!(out.contains("import \"licks.ctab\""));
        assert!(out.contains("barnumbers off"));
        assert!(out.contains("let riff = [3:0 2:0]"));
        assert!(out.contains("def echo(c) { ...c }"));
        assert!(out.contains("pickup { 5:0 }"));
        assert!(out.contains("loop 2 { 3:2 }"));
        assert!(out.contains("measure { (3:0) echo(riff) c.0 }"));
        assert_idempotent(src);
        assert_semantics_preserved(src);
    }
}
