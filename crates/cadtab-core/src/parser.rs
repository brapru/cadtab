//! Hand-rolled recursive-descent parser. This module is the engine: a
//! trivia-filtered token cursor with lookahead, span tracking, a diagnostic
//! sink, and recovery infrastructure (sync points + error nodes). It never
//! bails — on an unexpected token it reports, recovers, and emits an error node
//! so a half-typed document still yields a partial tree. Grammar productions
//! are added incrementally.

use crate::ast::{
    Block, Chord, ChordNote, Def, Duration, Ending, Event, EventKind, Expr, ExprKind, Fraction,
    Ident, IntLit, Item, ItemKind, Let, LoopBlock, Mark, MarkKind, Note, Position, Program, Repeat,
    Rest, Score, ScoreItem, ScoreItemKind, StringLit, Tie, TimeSig,
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

    /// Parse one top-level item. Returns `None` when the current token begins
    /// no item, so the caller can run its recovery path.
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
            Keyword::Def => {
                self.bump();
                self.parse_def()
            }
            Keyword::Let => {
                self.bump();
                self.parse_let()
            }
            // Score-level keywords are not valid at top level.
            _ => return None,
        };
        Some(Item::new(kind, self.span_from(start)))
    }

    /// `def IDENT ( params ) { body }` (the `def` keyword is consumed).
    fn parse_def(&mut self) -> ItemKind {
        let Some(name) = self.parse_ident() else {
            return ItemKind::Error;
        };
        let params = self.parse_params();
        let body = self.parse_block();
        ItemKind::Def(Def { name, params, body })
    }

    /// `( IDENT ( , IDENT )* )`, possibly empty.
    fn parse_params(&mut self) -> Vec<Ident> {
        let mut params = Vec::new();
        if !self.eat(TokenKind::LParen) {
            self.error_at(
                self.peek().span,
                format!("expected `(`, found {}", token_label(self.peek_kind())),
            );
            return params;
        }
        if self.eat(TokenKind::RParen) {
            return params;
        }
        while let Some(id) = self.parse_ident() {
            params.push(id);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen);
        params
    }

    /// `let IDENT = expr` (the `let` keyword is consumed).
    fn parse_let(&mut self) -> ItemKind {
        let Some(name) = self.parse_ident() else {
            return ItemKind::Error;
        };
        if !self.expect(TokenKind::Eq) {
            return ItemKind::Error;
        }
        let value = self.parse_expr();
        ItemKind::Let(Let { name, value })
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
            TokenKind::Keyword(Keyword::Loop) => {
                self.bump();
                let count = self
                    .parse_int_lit()
                    .unwrap_or_else(|| IntLit::new(0, Span::point(self.prev_end())));
                let body = self.parse_block();
                ScoreItemKind::Loop(LoopBlock { count, body })
            }
            _ => {
                return self.parse_event().map(|ev| {
                    let span = ev.span;
                    ScoreItem::new(ScoreItemKind::Event(ev), span)
                });
            }
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

    /// Parse events until `}`, EOF, or any token in `stop`. A token that begins
    /// no event is reported and skipped, so parsing recovers within the block.
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

    // --- events -----------------------------------------------------------

    /// `unit (~ unit)*` — a tie chain, left-associative. Returns `None` if the
    /// current token does not begin an event (so callers can dispatch/recover).
    fn parse_event(&mut self) -> Option<Event> {
        let mut node = self.parse_unit()?;
        while self.at(TokenKind::Tilde) {
            self.bump(); // `~`
            let right = self.parse_unit().unwrap_or_else(|| {
                let tok = self.peek();
                self.error_at(
                    tok.span,
                    format!("expected a note after `~`, found {}", token_label(tok.kind)),
                );
                Event::new(EventKind::Error, Span::point(tok.span.start))
            });
            let span = node.span.merge(right.span);
            node = Event::new(
                EventKind::Tie(Tie {
                    left: Box::new(node),
                    right: Box::new(right),
                }),
                span,
            );
        }
        Some(node)
    }

    /// `unit = chord | rest | (expr mark? duration?)`. A position head, or any
    /// head with a mark/duration, is a note; a bare call/ident/index is a phrase
    /// splice. Returns `None` if no event begins here.
    fn parse_unit(&mut self) -> Option<Event> {
        let start = self.peek().span.start;
        match self.peek_kind() {
            TokenKind::LBracket => {
                let chord = self.parse_chord();
                Some(Event::new(EventKind::Chord(chord), self.span_from(start)))
            }
            TokenKind::Ident if self.text(self.peek().span) == "r" => {
                self.bump(); // `r`
                let duration = self.parse_opt_duration();
                Some(Event::new(
                    EventKind::Rest(Rest { duration }),
                    self.span_from(start),
                ))
            }
            TokenKind::Int | TokenKind::Ident | TokenKind::LParen => {
                let head = self.parse_expr();
                let is_position = matches!(head.kind, ExprKind::Position(_));
                let mark = self.parse_opt_mark();
                let duration = self.parse_opt_duration();
                let kind = if is_position || mark.is_some() || duration.is_some() {
                    EventKind::Note(Note {
                        head,
                        mark,
                        duration,
                    })
                } else {
                    EventKind::Phrase(head)
                };
                Some(Event::new(kind, self.span_from(start)))
            }
            _ => None,
        }
    }

    /// `INT ":" INT`.
    fn parse_position(&mut self) -> Position {
        let start = self.peek().span.start;
        let string = self
            .parse_int_lit()
            .unwrap_or_else(|| IntLit::new(0, Span::point(self.prev_end())));
        self.expect(TokenKind::Colon);
        let fret = self
            .parse_int_lit()
            .unwrap_or_else(|| IntLit::new(0, Span::point(self.prev_end())));
        Position {
            string,
            fret,
            span: self.span_from(start),
        }
    }

    /// `[ chord_note+ ] duration?`.
    fn parse_chord(&mut self) -> Chord {
        self.bump(); // `[`
        let mut notes = Vec::new();
        while !self.at_eof() && !self.at(TokenKind::RBracket) {
            // Don't swallow an enclosing block's `}` while recovering.
            if self.at(TokenKind::RBrace) {
                break;
            }
            let before = self.pos;
            if self.at(TokenKind::Int) {
                notes.push(self.parse_chord_note());
            } else {
                let tok = self.peek();
                self.error_at(
                    tok.span,
                    format!("expected a note in chord, found {}", token_label(tok.kind)),
                );
                self.bump();
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.expect(TokenKind::RBracket);
        let duration = self.parse_opt_duration();
        Chord { notes, duration }
    }

    fn parse_chord_note(&mut self) -> ChordNote {
        let start = self.peek().span.start;
        let position = self.parse_position();
        let mark = self.parse_opt_mark();
        ChordNote {
            position,
            mark,
            span: self.span_from(start),
        }
    }

    /// `. mark_kind` where mark_kind ∈ {t,i,m,d,u}; absent or non-mark → `None`.
    fn parse_opt_mark(&mut self) -> Option<Mark> {
        if !(self.at(TokenKind::Dot) && matches!(self.peek2_kind(), TokenKind::Ident)) {
            return None;
        }
        let dot = self.bump(); // `.`
        let tok = self.bump(); // mark letter
        let label = self.text(tok.span).to_string();
        let kind = match label.as_str() {
            "t" => MarkKind::Thumb,
            "i" => MarkKind::Index,
            "m" => MarkKind::Middle,
            "d" => MarkKind::StrumDown,
            "u" => MarkKind::StrumUp,
            _ => {
                self.error_at(tok.span, format!("unknown mark `.{label}`"));
                return None;
            }
        };
        Some(Mark {
            kind,
            span: dot.span.merge(tok.span),
        })
    }

    /// `_ INT ("."...)` — denominator plus trailing augmentation dots.
    fn parse_opt_duration(&mut self) -> Option<Duration> {
        if !self.at(TokenKind::Underscore) {
            return None;
        }
        let start = self.peek().span.start;
        self.bump(); // `_`
        let denom = self
            .parse_int_lit()
            .unwrap_or_else(|| IntLit::new(0, Span::point(self.prev_end())));
        // Augmentation dots; `..` lexes as one `DotDot`, so count it as two.
        let mut dots = 0u8;
        loop {
            match self.peek_kind() {
                TokenKind::Dot => dots = dots.saturating_add(1),
                TokenKind::DotDot => dots = dots.saturating_add(2),
                _ => break,
            }
            self.bump();
        }
        Some(Duration {
            denom,
            dots,
            span: self.span_from(start),
        })
    }

    // --- expressions (Pratt) ---------------------------------------------

    /// An expression: a primary followed by `.N` index / `(args)` call postfixes.
    fn parse_expr(&mut self) -> Expr {
        let start = self.peek().span.start;
        let mut e = self.parse_primary();
        loop {
            match self.peek_kind() {
                // `.N` index — only when a number follows; `.mark` is note-level.
                TokenKind::Dot if matches!(self.peek2_kind(), TokenKind::Int) => {
                    self.bump(); // `.`
                    let index = self
                        .parse_int_lit()
                        .unwrap_or_else(|| IntLit::new(0, Span::point(self.prev_end())));
                    e = Expr::new(
                        ExprKind::Index {
                            base: Box::new(e),
                            index,
                        },
                        self.span_from(start),
                    );
                }
                TokenKind::LParen => {
                    self.bump(); // `(`
                    let args = self.parse_args();
                    self.expect(TokenKind::RParen);
                    e = Expr::new(
                        ExprKind::Call {
                            callee: Box::new(e),
                            args,
                        },
                        self.span_from(start),
                    );
                }
                _ => break,
            }
        }
        e
    }

    fn parse_primary(&mut self) -> Expr {
        let start = self.peek().span.start;
        match self.peek_kind() {
            TokenKind::Int => {
                let int = self
                    .parse_int_lit()
                    .unwrap_or_else(|| IntLit::new(0, Span::point(self.prev_end())));
                if self.eat(TokenKind::Colon) {
                    let fret = self
                        .parse_int_lit()
                        .unwrap_or_else(|| IntLit::new(0, Span::point(self.prev_end())));
                    let span = self.span_from(start);
                    Expr::new(
                        ExprKind::Position(Position {
                            string: int,
                            fret,
                            span,
                        }),
                        span,
                    )
                } else {
                    Expr::new(ExprKind::Int(int.value), int.span)
                }
            }
            TokenKind::Str => {
                let value = self.parse_string_lit().map(|s| s.value).unwrap_or_default();
                Expr::new(ExprKind::Str(value), self.span_from(start))
            }
            TokenKind::Ident => {
                let tok = self.bump();
                Expr::new(ExprKind::Ident(self.text(tok.span).to_string()), tok.span)
            }
            TokenKind::LBracket => {
                let chord = self.parse_chord();
                Expr::new(ExprKind::Chord(chord), self.span_from(start))
            }
            TokenKind::LParen => {
                self.bump(); // `(`
                let inner = self.parse_expr();
                self.expect(TokenKind::RParen);
                Expr::new(ExprKind::Paren(Box::new(inner)), self.span_from(start))
            }
            _ => {
                let tok = self.peek();
                self.error_at(
                    tok.span,
                    format!("expected an expression, found {}", token_label(tok.kind)),
                );
                Expr::new(ExprKind::Error, Span::point(start))
            }
        }
    }

    /// Comma-separated call arguments, each optionally `...`-spread.
    fn parse_args(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        if self.at(TokenKind::RParen) {
            return args;
        }
        loop {
            args.push(self.parse_arg());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        args
    }

    fn parse_arg(&mut self) -> Expr {
        if self.at(TokenKind::Ellipsis) {
            let start = self.peek().span.start;
            self.bump(); // `...`
            let inner = self.parse_expr();
            Expr::new(ExprKind::Spread(Box::new(inner)), self.span_from(start))
        } else {
            self.parse_expr()
        }
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

    // --- top-level declarations ------------------------------------

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

    // --- score / pickup / repeat / measure / endings ---------------

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

    // --- events (notes, chords, rests, ties) -----------------------

    use crate::ast::{EventKind, ExprKind, MarkKind};

    fn body_events(body: &str) -> Vec<Event> {
        let src = format!("score {{ {body} }}");
        let parsed = parse(&src);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        match parsed.program.items.into_iter().next().unwrap().kind {
            ItemKind::Score(s) => s
                .items
                .into_iter()
                .map(|it| match it.kind {
                    ScoreItemKind::Event(ev) => ev,
                    other => panic!("non-event: {other:?}"),
                })
                .collect(),
            other => panic!("{other:?}"),
        }
    }

    fn one_event(body: &str) -> Event {
        let mut v = body_events(body);
        assert_eq!(v.len(), 1);
        v.pop().unwrap()
    }

    #[test]
    fn bare_note_literal() {
        match one_event("3:2").kind {
            EventKind::Note(n) => {
                assert!(n.mark.is_none());
                assert!(n.duration.is_none());
                match n.head.kind {
                    ExprKind::Position(p) => assert_eq!((p.string.value, p.fret.value), (3, 2)),
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn note_with_mark_and_duration() {
        match one_event("3:2.t_8").kind {
            EventKind::Note(n) => {
                assert_eq!(n.mark.unwrap().kind, MarkKind::Thumb);
                let d = n.duration.unwrap();
                assert_eq!(d.denom.value, 8);
                assert_eq!(d.dots, 0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn dotted_durations_count_dots() {
        for (src, want) in [("3:2_4.", 1u8), ("3:2_4..", 2)] {
            match one_event(src).kind {
                EventKind::Note(n) => assert_eq!(n.duration.unwrap().dots, want),
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn strum_marks() {
        for (src, want) in [("1:0.d", MarkKind::StrumDown), ("1:0.u", MarkKind::StrumUp)] {
            match one_event(src).kind {
                EventKind::Note(n) => assert_eq!(n.mark.unwrap().kind, want),
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn chord_with_members_and_duration() {
        match one_event("[1:0.m 5:0.t]_4").kind {
            EventKind::Chord(c) => {
                assert_eq!(c.notes.len(), 2);
                assert_eq!(c.notes[0].mark.as_ref().unwrap().kind, MarkKind::Middle);
                assert_eq!(c.notes[1].mark.as_ref().unwrap().kind, MarkKind::Thumb);
                assert_eq!(c.duration.unwrap().denom.value, 4);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rest_with_and_without_duration() {
        assert!(matches!(
            one_event("r").kind,
            EventKind::Rest(r) if r.duration.is_none()
        ));
        match one_event("r_8").kind {
            EventKind::Rest(r) => assert_eq!(r.duration.unwrap().denom.value, 8),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn tie_joins_two_notes() {
        match one_event("3:2 ~ 3:2").kind {
            EventKind::Tie(t) => {
                assert!(matches!(t.left.kind, EventKind::Note(_)));
                assert!(matches!(t.right.kind, EventKind::Note(_)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn tie_chains_are_left_associative() {
        // a ~ b ~ c  =>  Tie(Tie(a, b), c)
        match one_event("3:2 ~ 3:4 ~ 3:5").kind {
            EventKind::Tie(outer) => {
                assert!(matches!(outer.left.kind, EventKind::Tie(_)));
                assert!(matches!(outer.right.kind, EventKind::Note(_)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn events_parse_inside_blocks() {
        // The pickup block now holds real events.
        let items = score_items("score { pickup { 2:0.i 1:0.t } }");
        match &items[0].kind {
            ScoreItemKind::Pickup(b) => assert_eq!(b.events.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_mark_reports() {
        let parsed = parse("score { 3:2.x }");
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.message.contains("unknown mark"))
        );
    }

    #[test]
    fn tie_without_rhs_reports_and_makes_error_node() {
        let parsed = parse("score { 3:2 ~ }");
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.message.contains("after `~`"))
        );
        match &parsed.program.items[0].kind {
            ItemKind::Score(s) => match &s.items[0].kind {
                ScoreItemKind::Event(ev) => match &ev.kind {
                    EventKind::Tie(t) => assert!(matches!(t.right.kind, EventKind::Error)),
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn events_body_snapshot() {
        let parsed = parse("score { 3:0.t [1:0.m 5:0.t]_4 r_8 3:2 ~ 3:2 }");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        insta::assert_debug_snapshot!(parsed.program);
    }

    // --- expressions (calls, index, spread) ------------------------

    /// The ident name of an expression, or `None`.
    fn ident_name(e: &Expr) -> Option<&str> {
        match &e.kind {
            ExprKind::Ident(n) => Some(n.as_str()),
            _ => None,
        }
    }

    #[test]
    fn call_event_with_position_args() {
        match one_event("hammer(3:0, 3:2)").kind {
            EventKind::Phrase(e) => match e.kind {
                ExprKind::Call { callee, args } => {
                    assert_eq!(ident_name(&callee), Some("hammer"));
                    assert_eq!(args.len(), 2);
                    assert!(matches!(args[0].kind, ExprKind::Position(_)));
                    assert!(matches!(args[1].kind, ExprKind::Position(_)));
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn call_with_no_args() {
        match one_event("bend(1:7)").kind {
            EventKind::Phrase(e) => match e.kind {
                ExprKind::Call { args, .. } => assert_eq!(args.len(), 1),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn spread_argument() {
        match one_event("forward_roll(...g_chord)").kind {
            EventKind::Phrase(e) => match e.kind {
                ExprKind::Call { args, .. } => {
                    assert_eq!(args.len(), 1);
                    match &args[0].kind {
                        ExprKind::Spread(inner) => assert_eq!(ident_name(inner), Some("g_chord")),
                        other => panic!("{other:?}"),
                    }
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn index_headed_note_with_mark() {
        // `chord.0 .t` → a note whose head is the index expression.
        match one_event("chord.0 .t").kind {
            EventKind::Note(n) => {
                assert_eq!(n.mark.unwrap().kind, MarkKind::Thumb);
                assert!(matches!(n.head.kind, ExprKind::Index { .. }));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bare_index_is_a_phrase() {
        match one_event("chord.0").kind {
            EventKind::Phrase(e) => match e.kind {
                ExprKind::Index { base, index } => {
                    assert_eq!(ident_name(&base), Some("chord"));
                    assert_eq!(index.value, 0);
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bare_ident_is_a_phrase() {
        match one_event("g_chord").kind {
            EventKind::Phrase(e) => assert_eq!(ident_name(&e), Some("g_chord")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn len_is_just_a_call() {
        match one_event("len(g_chord)").kind {
            EventKind::Phrase(e) => match e.kind {
                ExprKind::Call { callee, .. } => assert_eq!(ident_name(&callee), Some("len")),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn measure_with_technique_calls() {
        let items = score_items("score { measure { hammer(3:0, 3:2) bend(1:7) } }");
        match &items[0].kind {
            ScoreItemKind::Measure(b) => assert_eq!(b.events.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expression_event_snapshot() {
        let parsed = parse("score { forward_roll(...g_chord) chord.0 .t len(g_chord) }");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        insta::assert_debug_snapshot!(parsed.program);
    }

    // --- def / let / loop ------------------------------------------

    #[test]
    fn def_with_params_and_body() {
        let parsed = parse("def forward_roll(chord) { chord.0 .t chord.1 .i chord.2 .m }");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        match &parsed.program.items[0].kind {
            ItemKind::Def(d) => {
                assert_eq!(d.name.name, "forward_roll");
                assert_eq!(d.params.len(), 1);
                assert_eq!(d.params[0].name, "chord");
                assert_eq!(d.body.events.len(), 3);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn def_with_empty_params() {
        let parsed = parse("def alt() { 3:2 }");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        match &parsed.program.items[0].kind {
            ItemKind::Def(d) => assert!(d.params.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn let_binds_a_chord_value() {
        let parsed = parse("let g_chord = [3:0 2:0 1:0]");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        match &parsed.program.items[0].kind {
            ItemKind::Let(l) => {
                assert_eq!(l.name.name, "g_chord");
                match &l.value.kind {
                    ExprKind::Chord(c) => assert_eq!(c.notes.len(), 3),
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn let_missing_eq_reports() {
        let parsed = parse("let x 3");
        assert!(matches!(parsed.program.items[0].kind, ItemKind::Error));
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.message.contains("expected `=`"))
        );
    }

    #[test]
    fn loop_unroll_block() {
        let items = score_items("score { loop 2 { 3:2 3:4 } }");
        match &items[0].kind {
            ScoreItemKind::Loop(l) => {
                assert_eq!(l.count.value, 2);
                assert_eq!(l.body.events.len(), 2);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn def_call_loop_program_round_trips() {
        let parsed = parse(
            "def forward_roll(chord) { chord.0 .t chord.1 .i chord.2 .m }\n\
             let g_chord = [3:0 2:0 1:0]\n\
             score {\n  default 1/8\n  forward_roll(g_chord)\n  \
             loop 3 { forward_roll(g_chord) }\n  forward_roll(...g_chord)\n}",
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.program.items.len(), 3);
    }

    // --- valid-program capstone: the Cripple Creek example -------------------------

    const CRIPPLE_CREEK: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/cripple_creek.ctab"
    ));

    #[test]
    fn cripple_creek_parses_cleanly() {
        let parsed = parse(CRIPPLE_CREEK);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        // No error nodes anywhere a top-level item or score item.
        assert!(
            parsed
                .program
                .items
                .iter()
                .all(|it| !matches!(it.kind, ItemKind::Error))
        );
        insta::assert_debug_snapshot!(parsed.program);
    }

    // --- error-recovery corpus + multi-diagnostics -----------------

    fn diag_messages(src: &str) -> Vec<String> {
        parse(src)
            .diagnostics
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    fn diag_lines(src: &str) -> String {
        parse(src)
            .diagnostics
            .iter()
            .map(|d| format!("{}..{}: {}", d.span.start, d.span.end, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn collects_multiple_top_level_diagnostics() {
        // Two separate missing operands → two diagnostics, two error items.
        let parsed = parse("tempo\ncomposer");
        assert_eq!(parsed.program.items.len(), 2);
        assert!(
            parsed
                .program
                .items
                .iter()
                .all(|it| matches!(it.kind, ItemKind::Error))
        );
        assert!(parsed.diagnostics.len() >= 2);
    }

    #[test]
    fn valid_siblings_survive_a_broken_declaration() {
        // The broken `tempo` is reported, but title and instrument still parse.
        let parsed = parse("title \"ok\"\ntempo bad\ninstrument banjo");
        let kinds: Vec<_> = parsed.program.items.iter().map(|it| &it.kind).collect();
        assert!(kinds.iter().any(|k| matches!(k, ItemKind::Title(_))));
        assert!(kinds.iter().any(|k| matches!(k, ItemKind::Instrument(_))));
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.message.contains("expected number"))
        );
    }

    #[test]
    fn garbage_before_a_declaration_recovers() {
        // Stray numbers at top level are skipped to the next item start.
        let parsed = parse("4 5 title \"x\"");
        assert!(
            parsed
                .program
                .items
                .iter()
                .any(|it| matches!(it.kind, ItemKind::Title(_)))
        );
        assert!(!parsed.diagnostics.is_empty());
    }

    #[test]
    fn unclosed_score_brace_reports_once() {
        let msgs = diag_messages("score { time 4/4");
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("expected `}`"));
    }

    #[test]
    fn unclosed_chord_stops_at_block_brace() {
        // The chord recovery must not eat the score block's `}`.
        let parsed = parse("score { [1:0 5:0 }");
        match &parsed.program.items[0].kind {
            ItemKind::Score(s) => match &s.items[0].kind {
                ScoreItemKind::Event(ev) => match &ev.kind {
                    EventKind::Chord(c) => assert_eq!(c.notes.len(), 2),
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        // Just the missing `]` — the block's `}` was consumed normally.
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.message.contains("expected `]`"))
        );
        assert!(
            !parsed
                .diagnostics
                .iter()
                .any(|d| d.message.contains("expected `}`"))
        );
    }

    #[test]
    fn malformed_note_missing_fret_reports() {
        let parsed = parse("score { 3: }");
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.message.contains("expected number"))
        );
    }

    #[test]
    fn busy_program_collects_several_diagnostics() {
        let parsed = parse("tempo x\nscore { time 4 [1:0 }");
        assert!(parsed.diagnostics.len() >= 3, "{:?}", parsed.diagnostics);
    }

    #[test]
    fn parser_terminates_on_operator_soup() {
        // Pathological punctuation must not hang and must still reach EOF.
        let parsed = parse("score { : ~ . , = ] ) }");
        assert!(!parsed.diagnostics.is_empty());
    }

    #[test]
    fn recovery_diagnostics_snapshot() {
        insta::assert_snapshot!(diag_lines(
            "title\ntempo bad\nscore { time 4 measure { [1:0 5:0 } }"
        ));
    }
}
