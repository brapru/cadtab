//! Hand-rolled recursive-descent parser. This module is the engine: a
//! trivia-filtered token cursor with lookahead, span tracking, a diagnostic
//! sink, and recovery infrastructure (sync points + error nodes). It never
//! bails — on an unexpected token it reports, recovers, and emits an error node
//! so a half-typed document still yields a partial tree. Grammar productions
//! are added incrementally.

use crate::ast::{
    Block, Ending, Event, Fraction, Ident, IntLit, Item, ItemKind, Program, Repeat, Score,
    ScoreItem, ScoreItemKind, StringLit, TimeSig,
};
use crate::diagnostics::Diagnostic;
use crate::lexer::lex;
use crate::span::Span;
use crate::token::{Keyword, LexToken, TokenKind};

/// The result of parsing: a (possibly partial) program plus all diagnostics
/// (lexer + parser).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse `source` into a program, resiliently.
pub fn parse(source: &str) -> Parsed {
    Parser::new(source).parse_program()
}

struct Parser<'a> {
    source: &'a str,
    /// Significant tokens only (comments + lexer error tokens filtered out),
    /// always terminated by `Eof`.
    tokens: Vec<LexToken>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        let lexed = lex(source);
        let tokens: Vec<LexToken> = lexed
            .tokens
            .into_iter()
            .filter(|t| !matches!(t.kind, TokenKind::Comment | TokenKind::Error))
            .collect();
        Self {
            source,
            tokens,
            pos: 0,
            diagnostics: lexed.diagnostics,
        }
    }

    // --- cursor -----------------------------------------------------------

    /// The current token (clamped to the terminal `Eof`).
    fn peek(&self) -> LexToken {
        self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn at_eof(&self) -> bool {
        self.at(TokenKind::Eof)
    }

    /// Consume and return the current token (never advances past `Eof`).
    fn bump(&mut self) -> LexToken {
        let tok = self.peek();
        if !self.at_eof() {
            self.pos += 1;
        }
        tok
    }

    // --- spans & source ---------------------------------------------------

    /// End offset of the most recently consumed token (0 before any consume).
    fn prev_end(&self) -> u32 {
        if self.pos == 0 {
            0
        } else {
            self.tokens[self.pos - 1].span.end
        }
    }

    /// A span from `start` to the end of the last consumed token.
    fn span_from(&self, start: u32) -> Span {
        Span::new(start, self.prev_end())
    }

    // --- diagnostics & recovery ------------------------------------------

    fn error_at(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(span, message));
    }

    /// Does the current token begin a top-level item?
    fn at_top_level_start(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Keyword(
                Keyword::Title
                    | Keyword::Composer
                    | Keyword::Tempo
                    | Keyword::Instrument
                    | Keyword::Tuning
                    | Keyword::Capo
                    | Keyword::Import
                    | Keyword::Def
                    | Keyword::Let
                    | Keyword::Score
            )
        )
    }

    fn recover_to_top_level(&mut self) {
        while !self.at_eof() && !self.at_top_level_start() {
            self.bump();
        }
    }

    // --- productions ------------------------------------------------------

    fn parse_program(mut self) -> Parsed {
        let mut items = Vec::new();
        while !self.at_eof() {
            let before = self.pos;
            match self.parse_item() {
                Some(item) => items.push(item),
                None => {
                    let tok = self.peek();
                    let start = tok.span.start;
                    self.error_at(
                        tok.span,
                        format!("unexpected {} at top level", token_label(tok.kind)),
                    );
                    self.recover_to_top_level();
                    if self.pos == before {
                        // The offending token is itself a (mis-placed) sync
                        // point; consume it to guarantee progress.
                        self.bump();
                    }
                    items.push(Item::new(ItemKind::Error, self.span_from(start)));
                }
            }
            debug_assert!(self.pos > before, "parser made no progress");
        }
        Parsed {
            program: Program {
                items,
                span: Span::new(0, self.source.len() as u32),
            },
            diagnostics: self.diagnostics,
        }
    }

    /// Parse one top-level item. `score`/`def`/`let` are added in later
    /// sub-tasks; until then they fall through to the caller's recovery path.
    fn parse_item(&mut self) -> Option<Item> {
        let start = self.peek().span.start;
        let TokenKind::Keyword(kw) = self.peek_kind() else {
            return None;
        };
        let kind = match kw {
            Keyword::Title => {
                self.bump();
                self.string_decl(ItemKind::Title)
            }
            Keyword::Composer => {
                self.bump();
                self.string_decl(ItemKind::Composer)
            }
            Keyword::Capo => {
                self.bump();
                self.string_decl(ItemKind::Capo)
            }
            Keyword::Import => {
                self.bump();
                self.string_decl(ItemKind::Import)
            }
            Keyword::Tempo => {
                self.bump();
                match self.parse_int_lit() {
                    Some(n) => ItemKind::Tempo(n),
                    None => ItemKind::Error,
                }
            }
            Keyword::Instrument => {
                self.bump();
                self.ident_decl(ItemKind::Instrument)
            }
            Keyword::Tuning => {
                self.bump();
                self.ident_decl(ItemKind::Tuning)
            }
            Keyword::Score => {
                self.bump();
                self.parse_score()
            }
            // Not yet: def/let (T1.4f).
            _ => return None,
        };
        Some(Item::new(kind, self.span_from(start)))
    }

    /// `<keyword> STRING` → `f(string)`, or an error node if the string is
    /// missing (reported; the next item's keyword is left for recovery).
    fn string_decl(&mut self, f: impl FnOnce(StringLit) -> ItemKind) -> ItemKind {
        match self.parse_string_lit() {
            Some(s) => f(s),
            None => ItemKind::Error,
        }
    }

    /// `<keyword> IDENT` → `f(ident)`, or an error node if the ident is missing.
    fn ident_decl(&mut self, f: impl FnOnce(Ident) -> ItemKind) -> ItemKind {
        match self.parse_ident() {
            Some(id) => f(id),
            None => ItemKind::Error,
        }
    }

    /// A string literal (quotes stripped; escapes not decoded). Reports without
    /// consuming on mismatch.
    fn parse_string_lit(&mut self) -> Option<StringLit> {
        if self.at(TokenKind::Str) {
            let tok = self.bump();
            let raw = self.text(tok.span);
            let inner = raw.strip_prefix('"').unwrap_or(raw);
            let inner = inner.strip_suffix('"').unwrap_or(inner);
            Some(StringLit::new(inner, tok.span))
        } else {
            let tok = self.peek();
            self.error_at(
                tok.span,
                format!("expected string, found {}", token_label(tok.kind)),
            );
            None
        }
    }

    /// An integer literal. Overflowing values are clamped (and reported) so the
    /// node still exists for downstream resilience.
    fn parse_int_lit(&mut self) -> Option<IntLit> {
        if self.at(TokenKind::Int) {
            let tok = self.bump();
            match self.text(tok.span).parse::<u32>() {
                Ok(v) => Some(IntLit::new(v, tok.span)),
                Err(_) => {
                    self.error_at(tok.span, "number is too large");
                    Some(IntLit::new(u32::MAX, tok.span))
                }
            }
        } else {
            let tok = self.peek();
            self.error_at(
                tok.span,
                format!("expected number, found {}", token_label(tok.kind)),
            );
            None
        }
    }

    /// An identifier. Reports without consuming on mismatch.
    fn parse_ident(&mut self) -> Option<Ident> {
        if self.at(TokenKind::Ident) {
            let tok = self.bump();
            let name = self.text(tok.span).to_string();
            Some(Ident::new(name, tok.span))
        } else {
            let tok = self.peek();
            self.error_at(
                tok.span,
                format!("expected identifier, found {}", token_label(tok.kind)),
            );
            None
        }
    }

    // --- score block & its items -----------------------------------------

    /// `score { score_item* }` (the `score` keyword is already consumed).
    fn parse_score(&mut self) -> ItemKind {
        let lb = self.peek().span.start;
        if !self.eat(TokenKind::LBrace) {
            self.error_at(
                Span::point(lb),
                format!("expected `{{`, found {}", token_label(self.peek_kind())),
            );
            return ItemKind::Score(Score { items: Vec::new() });
        }
        let mut items = Vec::new();
        while !self.at_eof() && !self.at(TokenKind::RBrace) {
            let before = self.pos;
            match self.parse_score_item() {
                Some(item) => items.push(item),
                None => {
                    let tok = self.peek();
                    self.error_at(
                        tok.span,
                        format!("unexpected {} in score", token_label(tok.kind)),
                    );
                    self.bump();
                }
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.expect(TokenKind::RBrace);
        ItemKind::Score(Score { items })
    }

    fn parse_score_item(&mut self) -> Option<ScoreItem> {
        let start = self.peek().span.start;
        let kind = match self.peek_kind() {
            TokenKind::Keyword(Keyword::Time) => {
                self.bump();
                match self.parse_ratio() {
                    Some((num, den)) => ScoreItemKind::Time(TimeSig {
                        span: num.span.merge(den.span),
                        num,
                        den,
                    }),
                    None => ScoreItemKind::Error,
                }
            }
            TokenKind::Keyword(Keyword::Default) => {
                self.bump();
                match self.parse_ratio() {
                    Some((num, den)) => ScoreItemKind::Default(Fraction {
                        span: num.span.merge(den.span),
                        num,
                        den,
                    }),
                    None => ScoreItemKind::Error,
                }
            }
            TokenKind::Keyword(Keyword::Pickup) => {
                self.bump();
                ScoreItemKind::Pickup(self.parse_block())
            }
            TokenKind::Keyword(Keyword::Measure) => {
                self.bump();
                ScoreItemKind::Measure(self.parse_block())
            }
            TokenKind::Keyword(Keyword::Repeat) => {
                self.bump();
                self.parse_repeat()
            }
            // `loop` (T1.4f) and events (T1.4d) are not handled yet.
            _ => return None,
        };
        Some(ScoreItem::new(kind, self.span_from(start)))
    }

    /// `INT "/" INT`. Reports on a missing part.
    fn parse_ratio(&mut self) -> Option<(IntLit, IntLit)> {
        let num = self.parse_int_lit()?;
        if !self.expect(TokenKind::Slash) {
            return None;
        }
        let den = self.parse_int_lit()?;
        Some((num, den))
    }

    /// `repeat { event* ending(n){}* }` (the `repeat` keyword is consumed). The
    /// body events precede the voltas.
    fn parse_repeat(&mut self) -> ScoreItemKind {
        let lb = self.peek().span.start;
        if !self.eat(TokenKind::LBrace) {
            self.error_at(
                Span::point(lb),
                format!("expected `{{`, found {}", token_label(self.peek_kind())),
            );
            return ScoreItemKind::Repeat(Repeat {
                body: Vec::new(),
                endings: Vec::new(),
            });
        }
        let body = self.parse_events_until(&[TokenKind::Keyword(Keyword::Ending)]);
        let mut endings = Vec::new();
        while self.at_keyword(Keyword::Ending) {
            endings.push(self.parse_ending());
        }
        self.expect(TokenKind::RBrace);
        ScoreItemKind::Repeat(Repeat { body, endings })
    }

    /// `ending(N) { event* }` (the current token is `ending`).
    fn parse_ending(&mut self) -> Ending {
        let start = self.peek().span.start;
        self.bump(); // `ending`
        self.expect(TokenKind::LParen);
        let number = self
            .parse_int_lit()
            .unwrap_or_else(|| IntLit::new(0, Span::point(self.prev_end())));
        self.expect(TokenKind::RParen);
        let body = self.parse_block();
        Ending {
            number,
            body,
            span: self.span_from(start),
        }
    }

    /// `{ event* }`. A missing `{` is reported; the body is empty in that case.
    fn parse_block(&mut self) -> Block {
        let start = self.peek().span.start;
        if !self.eat(TokenKind::LBrace) {
            self.error_at(
                Span::point(start),
                format!("expected `{{`, found {}", token_label(self.peek_kind())),
            );
            return Block {
                events: Vec::new(),
                span: Span::point(start),
            };
        }
        let events = self.parse_events_until(&[]);
        self.expect(TokenKind::RBrace);
        Block {
            events,
            span: self.span_from(start),
        }
    }

    /// Parse events until `}`, EOF, or any token in `stop`. Event productions
    /// land in T1.4d; for now any event token is reported and skipped.
    fn parse_events_until(&mut self, stop: &[TokenKind]) -> Vec<Event> {
        let mut events = Vec::new();
        while !self.at_eof() && !self.at(TokenKind::RBrace) && !stop.contains(&self.peek_kind()) {
            let before = self.pos;
            match self.parse_event() {
                Some(ev) => events.push(ev),
                None => {
                    let tok = self.peek();
                    self.error_at(
                        tok.span,
                        format!("unexpected {} in block", token_label(tok.kind)),
                    );
                    self.bump();
                }
            }
            if self.pos == before {
                self.bump();
            }
        }
        events
    }

    /// Parse one event. Productions land in T1.4d.
    fn parse_event(&mut self) -> Option<Event> {
        None
    }
}

/// Cursor / recovery helpers that the grammar productions (added in later
/// sub-tasks) consume. They are part of the engine API and covered by tests.
#[allow(dead_code)]
impl Parser<'_> {
    /// The token after the current one (or `Eof`).
    fn peek2_kind(&self) -> TokenKind {
        self.tokens
            .get(self.pos + 1)
            .map_or(TokenKind::Eof, |t| t.kind)
    }

    fn at_keyword(&self, kw: Keyword) -> bool {
        self.peek_kind() == TokenKind::Keyword(kw)
    }

    /// Consume the current token iff it matches `kind`.
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Like [`eat`](Self::eat), but reports "expected …" on mismatch.
    fn expect(&mut self, kind: TokenKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        let tok = self.peek();
        self.error_at(
            tok.span,
            format!(
                "expected {}, found {}",
                token_label(kind),
                token_label(tok.kind)
            ),
        );
        false
    }

    /// Skip tokens until one of `sync` (or `Eof`) is current.
    fn recover_to_kinds(&mut self, sync: &[TokenKind]) {
        while !self.at_eof() && !sync.contains(&self.peek_kind()) {
            self.bump();
        }
    }

    /// The source text covered by `span`.
    fn text(&self, span: Span) -> &str {
        &self.source[span.start as usize..span.end as usize]
    }
}

/// A human-facing label for a token kind, used in diagnostics.
fn token_label(kind: TokenKind) -> String {
    use TokenKind::*;
    match kind {
        Int => "number".into(),
        Str => "string".into(),
        Ident => "identifier".into(),
        Keyword(kw) => format!("keyword `{}`", kw.as_str()),
        Colon => "`:`".into(),
        Underscore => "`_`".into(),
        Tilde => "`~`".into(),
        Dot => "`.`".into(),
        DotDot => "`..`".into(),
        Ellipsis => "`...`".into(),
        Slash => "`/`".into(),
        Eq => "`=`".into(),
        Comma => "`,`".into(),
        LBracket => "`[`".into(),
        RBracket => "`]`".into(),
        LBrace => "`{`".into(),
        RBrace => "`}`".into(),
        LParen => "`(`".into(),
        RParen => "`)`".into(),
        Eof => "end of input".into(),
        Comment | Error => "unexpected token".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source_parses_to_empty_program() {
        let parsed = parse("");
        assert!(parsed.program.items.is_empty());
        assert!(parsed.diagnostics.is_empty());
        assert_eq!(parsed.program.span, Span::new(0, 0));
    }

    #[test]
    fn trivia_only_parses_to_empty_program() {
        let parsed = parse("  // a comment\n/* block */\n");
        assert!(parsed.program.items.is_empty());
        assert!(parsed.diagnostics.is_empty());
    }

    #[test]
    fn lexer_error_is_carried_through_without_parser_noise() {
        // `@` is a lexer error token (filtered out); the parser adds nothing.
        let parsed = parse("@");
        assert!(parsed.program.items.is_empty());
        assert_eq!(parsed.diagnostics.len(), 1);
    }

    #[test]
    fn unexpected_top_level_token_yields_error_node_and_diagnostic() {
        let parsed = parse("3:0");
        assert_eq!(parsed.program.items.len(), 1);
        assert!(matches!(parsed.program.items[0].kind, ItemKind::Error));
        assert_eq!(parsed.diagnostics.len(), 1);
        assert!(parsed.diagnostics[0].message.contains("unexpected"));
    }

    #[test]
    fn parser_always_terminates_on_stuck_token() {
        // A lone operator that recovery treats as a sync candidate must still
        // make progress (no infinite loop).
        let parsed = parse("=");
        assert_eq!(parsed.program.items.len(), 1);
        assert!(matches!(parsed.program.items[0].kind, ItemKind::Error));
    }

    // --- infra unit tests -------------------------------------------------

    #[test]
    fn cursor_peek_lookahead_and_bump() {
        let mut p = Parser::new("score : 4");
        assert_eq!(p.peek_kind(), TokenKind::Keyword(Keyword::Score));
        assert_eq!(p.peek2_kind(), TokenKind::Colon);
        assert!(p.at_keyword(Keyword::Score));
        p.bump();
        assert_eq!(p.peek_kind(), TokenKind::Colon);
        p.bump();
        assert_eq!(p.peek_kind(), TokenKind::Int);
        p.bump();
        assert!(p.at_eof());
        // bump past Eof is a no-op
        p.bump();
        assert!(p.at_eof());
    }

    #[test]
    fn eat_matches_then_advances() {
        let mut p = Parser::new("score");
        assert!(!p.eat(TokenKind::Colon)); // no match, no advance
        assert!(p.at_keyword(Keyword::Score));
        assert!(p.eat(TokenKind::Keyword(Keyword::Score)));
        assert!(p.at_eof());
        assert!(p.diagnostics.is_empty());
    }

    #[test]
    fn expect_reports_on_mismatch_without_advancing() {
        let mut p = Parser::new(":");
        let ok = p.expect(TokenKind::Keyword(Keyword::Score));
        assert!(!ok);
        assert_eq!(p.diagnostics.len(), 1);
        assert!(p.at(TokenKind::Colon)); // did not advance
        assert!(
            p.diagnostics[0]
                .message
                .contains("expected keyword `score`")
        );
    }

    #[test]
    fn recover_to_kinds_stops_at_sync() {
        let mut p = Parser::new("3 : 0 , 4");
        p.recover_to_kinds(&[TokenKind::Comma]);
        assert!(p.at(TokenKind::Comma));
    }

    #[test]
    fn text_slices_source_for_a_span() {
        let p = Parser::new("banjo");
        let tok = p.peek();
        assert_eq!(p.text(tok.span), "banjo");
    }

    // --- T1.4b: top-level declarations ------------------------------------

    use crate::ast::IntLit;

    fn only_item(src: &str) -> ItemKind {
        let parsed = parse(src);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.program.items.len(), 1);
        parsed.program.items.into_iter().next().unwrap().kind
    }

    #[test]
    fn string_declarations() {
        match only_item("title \"Cripple Creek\"") {
            ItemKind::Title(s) => assert_eq!(s.value, "Cripple Creek"),
            other => panic!("{other:?}"),
        }
        match only_item("composer \"trad.\"") {
            ItemKind::Composer(s) => assert_eq!(s.value, "trad."),
            other => panic!("{other:?}"),
        }
        match only_item("capo \"5th string @ 2\"") {
            ItemKind::Capo(s) => assert_eq!(s.value, "5th string @ 2"),
            other => panic!("{other:?}"),
        }
        match only_item("import \"rolls.ctab\"") {
            ItemKind::Import(s) => assert_eq!(s.value, "rolls.ctab"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn tempo_declaration() {
        match only_item("tempo 130") {
            ItemKind::Tempo(IntLit { value, .. }) => assert_eq!(value, 130),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn instrument_and_tuning_declarations() {
        match only_item("instrument banjo") {
            ItemKind::Instrument(id) => assert_eq!(id.name, "banjo"),
            other => panic!("{other:?}"),
        }
        match only_item("tuning openG") {
            ItemKind::Tuning(id) => assert_eq!(id.name, "openG"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn item_span_covers_keyword_through_operand() {
        let parsed = parse("tempo 130");
        assert_eq!(parsed.program.items[0].span, Span::new(0, 9));
    }

    #[test]
    fn multiple_declarations_parse_in_order() {
        let parsed = parse("title \"X\"\ninstrument banjo\ntempo 90");
        assert!(parsed.diagnostics.is_empty());
        assert_eq!(parsed.program.items.len(), 3);
        assert!(matches!(parsed.program.items[0].kind, ItemKind::Title(_)));
        assert!(matches!(
            parsed.program.items[1].kind,
            ItemKind::Instrument(_)
        ));
        assert!(matches!(parsed.program.items[2].kind, ItemKind::Tempo(_)));
    }

    #[test]
    fn missing_operand_reports_and_makes_error_node() {
        let parsed = parse("title");
        assert_eq!(parsed.program.items.len(), 1);
        assert!(matches!(parsed.program.items[0].kind, ItemKind::Error));
        assert_eq!(parsed.diagnostics.len(), 1);
        assert!(parsed.diagnostics[0].message.contains("expected string"));
    }

    #[test]
    fn missing_operand_recovers_to_next_declaration() {
        // `title` has no string; the next keyword starts a fresh, valid item.
        let parsed = parse("title\ninstrument banjo");
        assert_eq!(parsed.program.items.len(), 2);
        assert!(matches!(parsed.program.items[0].kind, ItemKind::Error));
        assert!(matches!(
            parsed.program.items[1].kind,
            ItemKind::Instrument(_)
        ));
        assert_eq!(parsed.diagnostics.len(), 1);
    }

    #[test]
    fn tempo_with_non_number_reports() {
        let parsed = parse("tempo banjo");
        assert!(matches!(parsed.program.items[0].kind, ItemKind::Error));
        assert!(parsed.diagnostics[0].message.contains("expected number"));
    }

    #[test]
    fn metadata_header_snapshot() {
        let parsed = parse(
            "title \"Cripple Creek\"\ncomposer \"trad.\"\ntempo 130\n\
             instrument banjo\ntuning openG\ncapo \"5th string @ 2\"",
        );
        assert!(parsed.diagnostics.is_empty());
        insta::assert_debug_snapshot!(parsed.program);
    }

    // --- T1.4c: score / pickup / repeat / measure / endings ---------------

    use crate::ast::{ScoreItemKind, TimeSig};

    fn score_items(src: &str) -> Vec<ScoreItem> {
        let parsed = parse(src);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.program.items.len(), 1);
        match parsed.program.items.into_iter().next().unwrap().kind {
            ItemKind::Score(s) => s.items,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn empty_score_block() {
        let items = score_items("score { }");
        assert!(items.is_empty());
    }

    #[test]
    fn time_and_default_settings() {
        let items = score_items("score { time 4/4 default 1/8 }");
        assert_eq!(items.len(), 2);
        match &items[0].kind {
            ScoreItemKind::Time(TimeSig { num, den, .. }) => {
                assert_eq!((num.value, den.value), (4, 4));
            }
            other => panic!("{other:?}"),
        }
        match &items[1].kind {
            ScoreItemKind::Default(f) => assert_eq!((f.num.value, f.den.value), (1, 8)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn pickup_and_measure_blocks_are_recognized() {
        let items = score_items("score { pickup { } measure { } }");
        assert!(matches!(items[0].kind, ScoreItemKind::Pickup(_)));
        assert!(matches!(items[1].kind, ScoreItemKind::Measure(_)));
    }

    #[test]
    fn repeat_with_endings_splits_body_and_voltas() {
        let items = score_items("score { repeat { ending(1) { } ending(2) { } } }");
        assert_eq!(items.len(), 1);
        match &items[0].kind {
            ScoreItemKind::Repeat(r) => {
                assert!(r.body.is_empty());
                assert_eq!(r.endings.len(), 2);
                assert_eq!(r.endings[0].number.value, 1);
                assert_eq!(r.endings[1].number.value, 2);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nested_blocks_carry_spans() {
        // The block span runs from its `{` (offset 16) to its `}` (offset 19).
        let items = score_items("score { measure { } }");
        match &items[0].kind {
            ScoreItemKind::Measure(b) => assert_eq!(b.span, Span::new(16, 19)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn score_missing_close_brace_reports_but_recovers() {
        let parsed = parse("score { time 4/4");
        // The score item still parsed; a single "expected `}`" is reported.
        match &parsed.program.items[0].kind {
            ItemKind::Score(s) => assert_eq!(s.items.len(), 1),
            other => panic!("{other:?}"),
        }
        assert_eq!(parsed.diagnostics.len(), 1);
        assert!(parsed.diagnostics[0].message.contains("expected `}`"));
    }

    #[test]
    fn malformed_time_reports_and_continues() {
        let parsed = parse("score { time 4 }");
        match &parsed.program.items[0].kind {
            ItemKind::Score(s) => assert!(matches!(s.items[0].kind, ScoreItemKind::Error)),
            other => panic!("{other:?}"),
        }
        assert!(!parsed.diagnostics.is_empty());
    }

    #[test]
    fn score_block_snapshot() {
        let parsed = parse(
            "score {\n  time 4/4\n  default 1/8\n  pickup { }\n  \
             repeat { ending(1) { } ending(2) { } }\n  measure { }\n}",
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        insta::assert_debug_snapshot!(parsed.program);
    }
}
