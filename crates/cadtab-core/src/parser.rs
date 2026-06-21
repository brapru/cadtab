//! Hand-rolled recursive-descent parser. This module is the engine: a
//! trivia-filtered token cursor with lookahead, span tracking, a diagnostic
//! sink, and recovery infrastructure (sync points + error nodes). It never
//! bails — on an unexpected token it reports, recovers, and emits an error node
//! so a half-typed document still yields a partial tree. Grammar productions
//! are added incrementally.

use crate::ast::{Item, ItemKind, Program};
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

    /// Parse one top-level item. Productions are added in later sub-tasks; for
    /// now nothing is recognized, so the caller's recovery path handles input.
    fn parse_item(&mut self) -> Option<Item> {
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
}
