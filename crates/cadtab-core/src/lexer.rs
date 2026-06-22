//! Hand-rolled lexer. Scans source into [`LexToken`]s + diagnostics without
//! bailing: unrecognized input becomes an `Error` token so the stream always
//! reaches `Eof`. Comments are emitted as trivia tokens; the parser skips them.

use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::token::{Keyword, LexToken, TokenKind};

/// The result of lexing: the full token stream (ending in `Eof`) plus any
/// diagnostics gathered along the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexed {
    pub tokens: Vec<LexToken>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Convenience entry point: lex `src` to completion.
pub fn lex(src: &str) -> Lexed {
    Lexer::new(src).tokenize()
}

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    /// Byte at the cursor, if any.
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// Byte `off` positions past the cursor, if any.
    fn peek_at(&self, off: usize) -> Option<u8> {
        self.bytes.get(self.pos + off).copied()
    }

    pub fn tokenize(mut self) -> Lexed {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            let start = self.pos as u32;
            let Some(b) = self.peek() else {
                tokens.push(LexToken::new(TokenKind::Eof, Span::point(start)));
                break;
            };
            let kind = match b {
                b'/' if self.peek_at(1) == Some(b'/') => self.line_comment(),
                b'/' if self.peek_at(1) == Some(b'*') => self.block_comment(start),
                b'"' => self.string(start),
                b if b.is_ascii_digit() => self.number(),
                b if b.is_ascii_alphabetic() => self.ident(start),
                b':' => self.single(TokenKind::Colon),
                b'_' => self.single(TokenKind::Underscore),
                b'~' => self.single(TokenKind::Tilde),
                b'/' => self.single(TokenKind::Slash),
                b'.' => self.dot(),
                b',' => self.single(TokenKind::Comma),
                b'=' => self.single(TokenKind::Eq),
                b'[' => self.single(TokenKind::LBracket),
                b']' => self.single(TokenKind::RBracket),
                b'{' => self.single(TokenKind::LBrace),
                b'}' => self.single(TokenKind::RBrace),
                b'(' => self.single(TokenKind::LParen),
                b')' => self.single(TokenKind::RParen),
                _ => self.error_run(start),
            };
            tokens.push(LexToken::new(kind, Span::new(start, self.pos as u32)));
        }
        Lexed {
            tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// `// …` to end of line (the newline is not part of the comment).
    fn line_comment(&mut self) -> TokenKind {
        self.pos += 2;
        while let Some(b) = self.peek() {
            if b == b'\n' {
                break;
            }
            self.pos += 1;
        }
        TokenKind::Comment
    }

    /// `/* … */`, non-nesting (first `*/` closes). Unterminated → diagnostic +
    /// a comment token spanning to EOF (resilient).
    fn block_comment(&mut self, start: u32) -> TokenKind {
        self.pos += 2;
        loop {
            match self.peek() {
                None => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            Span::new(start, self.pos as u32),
                            "unterminated block comment",
                        )
                        .with_help("add `*/` to close the comment"),
                    );
                    break;
                }
                Some(b'*') if self.peek_at(1) == Some(b'/') => {
                    self.pos += 2;
                    break;
                }
                Some(_) => self.pos += 1,
            }
        }
        TokenKind::Comment
    }

    /// Emit a single-byte token and advance past it.
    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.pos += 1;
        kind
    }

    /// `...` (spread), `..` (reserved), or a bare `.` — maximal munch.
    fn dot(&mut self) -> TokenKind {
        if self.peek_at(1) == Some(b'.') {
            if self.peek_at(2) == Some(b'.') {
                self.pos += 3;
                TokenKind::Ellipsis
            } else {
                self.pos += 2;
                TokenKind::DotDot
            }
        } else {
            self.pos += 1;
            TokenKind::Dot
        }
    }

    /// A run of ASCII digits.
    fn number(&mut self) -> TokenKind {
        while matches!(self.peek(), Some(b) if b.is_ascii_digit()) {
            self.pos += 1;
        }
        TokenKind::Int
    }

    /// An identifier starting with an ASCII letter, then letters/digits, plus
    /// `_` only when followed by a letter (so `_<digit>` stays a duration suffix
    /// and `r_8` lexes as `r` then `_8`). Recognized keywords lower to
    /// [`TokenKind::Keyword`].
    fn ident(&mut self, start: u32) -> TokenKind {
        self.pos += 1; // leading letter
        loop {
            match self.peek() {
                Some(b) if b.is_ascii_alphanumeric() => self.pos += 1,
                Some(b'_') if matches!(self.peek_at(1), Some(c) if c.is_ascii_alphabetic()) => {
                    self.pos += 1;
                }
                _ => break,
            }
        }
        let text = &self.src[start as usize..self.pos];
        match Keyword::from_ident(text) {
            Some(kw) => TokenKind::Keyword(kw),
            None => TokenKind::Ident,
        }
    }

    /// A double-quoted, single-line string with `\`-escapes (the escape is not
    /// decoded here). A newline or EOF before the closing `"` is unterminated →
    /// diagnostic + a resilient `Str` token spanning what was read.
    fn string(&mut self, start: u32) -> TokenKind {
        self.pos += 1; // opening quote
        loop {
            match self.peek() {
                None | Some(b'\n') => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            Span::new(start, self.pos as u32),
                            "unterminated string literal",
                        )
                        .with_help("add a closing `\"`"),
                    );
                    break;
                }
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    self.pos += 1;
                    // Skip the escaped byte, unless the line/input ends first.
                    if !matches!(self.peek(), None | Some(b'\n')) {
                        self.pos += 1;
                    }
                }
                Some(_) => self.pos += 1,
            }
        }
        TokenKind::Str
    }

    /// True for any byte that begins a recognized token (mirrors the dispatch).
    fn is_token_start(b: u8) -> bool {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'/' | b'"'
                    | b':'
                    | b'_'
                    | b'~'
                    | b'.'
                    | b','
                    | b'='
                    | b'['
                    | b']'
                    | b'{'
                    | b'}'
                    | b'('
                    | b')'
            )
    }

    /// Consume a run of unrecognized characters into one `Error` token + one
    /// diagnostic. Stops at whitespace or the next valid token start; advances
    /// by whole UTF-8 chars so spans stay on char boundaries.
    fn error_run(&mut self, start: u32) -> TokenKind {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() || Self::is_token_start(b) {
                break;
            }
            let len = self.src[self.pos..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
            self.pos += len;
        }
        let span = Span::new(start, self.pos as u32);
        let text = &self.src[start as usize..self.pos];
        let plural = if text.chars().count() > 1 { "s" } else { "" };
        self.diagnostics.push(
            Diagnostic::error(span, format!("unexpected character{plural} `{text}`"))
                .with_help("remove or replace the unexpected input"),
        );
        TokenKind::Error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Severity;

    fn kinds(lexed: &Lexed) -> Vec<TokenKind> {
        lexed.tokens.iter().map(|t| t.kind).collect()
    }

    /// One token per line as `Kind start..end` — a compact, reviewable snapshot.
    fn compact(lexed: &Lexed) -> String {
        lexed
            .tokens
            .iter()
            .map(|t| format!("{:?} {}..{}", t.kind, t.span.start, t.span.end))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn empty_input_yields_only_eof() {
        let lexed = lex("");
        assert_eq!(kinds(&lexed), vec![TokenKind::Eof]);
        assert_eq!(lexed.tokens[0].span, Span::point(0));
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn whitespace_only_emits_no_tokens_but_eof() {
        let lexed = lex("  \t\n  ");
        assert_eq!(kinds(&lexed), vec![TokenKind::Eof]);
        assert_eq!(lexed.tokens[0].span, Span::point(6));
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn line_comment_spans_to_newline_not_including_it() {
        let lexed = lex("  // hi\n");
        assert_eq!(kinds(&lexed), vec![TokenKind::Comment, TokenKind::Eof]);
        assert_eq!(lexed.tokens[0].span, Span::new(2, 7)); // "// hi"
        assert_eq!(lexed.tokens[1].span, Span::point(8)); // after newline
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn line_comment_at_eof_without_newline() {
        let lexed = lex("// end");
        assert_eq!(lexed.tokens[0].span, Span::new(0, 6));
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn block_comment_spans_across_newlines() {
        let lexed = lex("/* a\n b */x");
        assert_eq!(
            kinds(&lexed),
            vec![TokenKind::Comment, TokenKind::Ident, TokenKind::Eof]
        );
        assert_eq!(lexed.tokens[0].span, Span::new(0, 10)); // "/* a\n b */"
        assert_eq!(lexed.tokens[1].span, Span::new(10, 11)); // 'x'
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn block_comment_is_not_nested() {
        // First `*/` closes; the trailing ` */` is unhandled chars, not comment.
        let lexed = lex("/* outer /* inner */");
        assert_eq!(lexed.tokens[0].kind, TokenKind::Comment);
        assert_eq!(lexed.tokens[0].span, Span::new(0, 20));
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn unterminated_block_comment_reports_diagnostic() {
        let lexed = lex("/* oops");
        assert_eq!(kinds(&lexed), vec![TokenKind::Comment, TokenKind::Eof]);
        assert_eq!(lexed.tokens[0].span, Span::new(0, 7));
        assert_eq!(lexed.diagnostics.len(), 1);
        assert_eq!(lexed.diagnostics[0].message, "unterminated block comment");
    }

    #[test]
    fn unknown_char_advances_one_full_utf8_char() {
        // 'é' is two bytes; it must become one Error token, not two.
        let lexed = lex("é");
        assert_eq!(kinds(&lexed), vec![TokenKind::Error, TokenKind::Eof]);
        assert_eq!(lexed.tokens[0].span, Span::new(0, 2));
    }

    #[test]
    fn unexpected_char_reports_diagnostic() {
        let lexed = lex("@");
        assert_eq!(kinds(&lexed), vec![TokenKind::Error, TokenKind::Eof]);
        assert_eq!(lexed.tokens[0].span, Span::new(0, 1));
        assert_eq!(lexed.diagnostics.len(), 1);
        assert_eq!(lexed.diagnostics[0].severity, Severity::Error);
        assert_eq!(lexed.diagnostics[0].message, "unexpected character `@`");
        assert!(lexed.diagnostics[0].help.is_some());
    }

    #[test]
    fn unexpected_char_run_coalesces() {
        // A contiguous garbage run is one Error token + one diagnostic.
        let lexed = lex("@$%");
        assert_eq!(kinds(&lexed), vec![TokenKind::Error, TokenKind::Eof]);
        assert_eq!(lexed.tokens[0].span, Span::new(0, 3));
        assert_eq!(lexed.diagnostics.len(), 1);
        assert_eq!(lexed.diagnostics[0].message, "unexpected characters `@$%`");
    }

    #[test]
    fn error_run_stops_at_whitespace_and_token_starts() {
        // `@ $` → two runs; `@3` → one run then an Int.
        let split = lex("@ $");
        assert_eq!(
            kinds(&split),
            vec![TokenKind::Error, TokenKind::Error, TokenKind::Eof]
        );
        assert_eq!(split.diagnostics.len(), 2);

        let abuts = lex("@3");
        assert_eq!(
            kinds(&abuts),
            vec![TokenKind::Error, TokenKind::Int, TokenKind::Eof]
        );
        assert_eq!(abuts.tokens[0].span, Span::new(0, 1)); // stopped at the digit
        assert_eq!(abuts.diagnostics.len(), 1);
    }

    #[test]
    fn error_run_keeps_multibyte_char_boundaries() {
        // '€' is three bytes; one run, one diagnostic, boundary-aligned span.
        let lexed = lex("€");
        assert_eq!(kinds(&lexed), vec![TokenKind::Error, TokenKind::Eof]);
        assert_eq!(lexed.tokens[0].span, Span::new(0, 3));
        assert_eq!(lexed.diagnostics.len(), 1);
    }

    #[test]
    fn lexer_stays_resilient_amid_errors() {
        // Valid tokens around garbage still lex; the stream reaches Eof.
        let lexed = lex("score @ 3:0");
        assert_eq!(
            kinds(&lexed),
            vec![
                TokenKind::Keyword(Keyword::Score),
                TokenKind::Error,
                TokenKind::Int,
                TokenKind::Colon,
                TokenKind::Int,
                TokenKind::Eof,
            ]
        );
        assert_eq!(lexed.diagnostics.len(), 1);
    }

    #[test]
    fn comments_and_whitespace_snapshot() {
        let lexed = lex("  // line\n/* block\n   spans */  \n");
        assert!(lexed.diagnostics.is_empty());
        insta::assert_debug_snapshot!(lexed.tokens);
    }

    #[test]
    fn integers() {
        let lexed = lex("0 130 42");
        assert_eq!(
            kinds(&lexed),
            vec![
                TokenKind::Int,
                TokenKind::Int,
                TokenKind::Int,
                TokenKind::Eof
            ]
        );
        assert_eq!(lexed.tokens[0].span, Span::new(0, 1));
        assert_eq!(lexed.tokens[1].span, Span::new(2, 5));
        assert_eq!(lexed.tokens[2].span, Span::new(6, 8));
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn identifiers_and_keywords() {
        let lexed = lex("score banjo forward_roll abc123 r");
        assert_eq!(
            kinds(&lexed),
            vec![
                TokenKind::Keyword(Keyword::Score),
                TokenKind::Ident, // banjo
                TokenKind::Ident, // forward_roll
                TokenKind::Ident, // abc123
                TokenKind::Ident, // r — contextual, not a keyword
                TokenKind::Eof,
            ]
        );
        assert_eq!(lexed.tokens[2].span, Span::new(12, 24)); // forward_roll
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn underscore_joins_idents_only_before_a_letter() {
        // `_letter` joins the name; `_digit` is a duration suffix.
        assert_eq!(
            kinds(&lex("forward_roll")),
            vec![TokenKind::Ident, TokenKind::Eof]
        );
        assert_eq!(
            kinds(&lex("g_chord")),
            vec![TokenKind::Ident, TokenKind::Eof]
        );
        let rest = lex("r_8");
        assert_eq!(
            kinds(&rest),
            vec![
                TokenKind::Ident,
                TokenKind::Underscore,
                TokenKind::Int,
                TokenKind::Eof
            ]
        );
        assert_eq!(rest.tokens[0].span, Span::new(0, 1)); // just `r`
    }

    #[test]
    fn underscore_does_not_start_an_identifier() {
        // `_8` is the duration lead + int, not an identifier `_8`.
        let lexed = lex("_8 g_chord");
        assert_eq!(
            kinds(&lexed),
            vec![
                TokenKind::Underscore, // `_`
                TokenKind::Int,        // 8
                TokenKind::Ident,      // g_chord (contains, not leads, `_`)
                TokenKind::Eof,
            ]
        );
        assert_eq!(lexed.tokens[2].span, Span::new(3, 10));
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn strings_with_escapes_and_unicode() {
        let lexed = lex(r#""hello" "a\"b" "café""#);
        assert_eq!(
            kinds(&lexed),
            vec![
                TokenKind::Str,
                TokenKind::Str,
                TokenKind::Str,
                TokenKind::Eof
            ]
        );
        assert_eq!(lexed.tokens[0].span, Span::new(0, 7)); // "hello"
        assert_eq!(lexed.tokens[1].span, Span::new(8, 14)); // "a\"b" — escape doesn't close
        assert_eq!(lexed.tokens[2].span, Span::new(15, 22)); // "café" — é is 2 bytes
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn unterminated_string_reports_diagnostic() {
        let lexed = lex("\"oops");
        assert_eq!(kinds(&lexed), vec![TokenKind::Str, TokenKind::Eof]);
        assert_eq!(lexed.tokens[0].span, Span::new(0, 5));
        assert_eq!(lexed.diagnostics.len(), 1);
        assert_eq!(lexed.diagnostics[0].message, "unterminated string literal");
    }

    #[test]
    fn string_does_not_span_newline() {
        let lexed = lex("\"oops\nscore");
        assert_eq!(
            kinds(&lexed),
            vec![
                TokenKind::Str,
                TokenKind::Keyword(Keyword::Score),
                TokenKind::Eof
            ]
        );
        assert_eq!(lexed.tokens[0].span, Span::new(0, 5)); // up to the newline
        assert_eq!(lexed.diagnostics.len(), 1);
    }

    #[test]
    fn metadata_header_snapshot() {
        let lexed = lex("title \"Syntax Showcase\"\ncomposer \"cadtab\"\ntempo 130");
        assert!(lexed.diagnostics.is_empty());
        insta::assert_debug_snapshot!(lexed.tokens);
    }

    #[test]
    fn note_literal_and_duration() {
        let lexed = lex("3:0 5:2_8");
        assert_eq!(
            kinds(&lexed),
            vec![
                TokenKind::Int,        // 3
                TokenKind::Colon,      // :
                TokenKind::Int,        // 0
                TokenKind::Int,        // 5
                TokenKind::Colon,      // :
                TokenKind::Int,        // 2
                TokenKind::Underscore, // _
                TokenKind::Int,        // 8
                TokenKind::Eof,
            ]
        );
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn tie_operator() {
        let lexed = lex("3:2 ~ 3:2");
        assert_eq!(
            kinds(&lexed),
            vec![
                TokenKind::Int,
                TokenKind::Colon,
                TokenKind::Int,
                TokenKind::Tilde,
                TokenKind::Int,
                TokenKind::Colon,
                TokenKind::Int,
                TokenKind::Eof,
            ]
        );
        assert_eq!(lexed.tokens[3].span, Span::new(4, 5)); // `~`
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn underscore_is_its_own_token() {
        // `_8` is two tokens; the underscore no longer falls through to Error.
        let lexed = lex("_8");
        assert_eq!(
            kinds(&lexed),
            vec![TokenKind::Underscore, TokenKind::Int, TokenKind::Eof]
        );
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn time_signature_slash_vs_comments() {
        // A lone `/` is Slash; `//` and `/*` still scan as comments.
        let lexed = lex("4/4");
        assert_eq!(
            kinds(&lexed),
            vec![
                TokenKind::Int,
                TokenKind::Slash,
                TokenKind::Int,
                TokenKind::Eof
            ]
        );
        assert_eq!(lexed.tokens[1].span, Span::new(1, 2)); // `/`

        assert_eq!(
            kinds(&lex("a // c")),
            vec![TokenKind::Ident, TokenKind::Comment, TokenKind::Eof]
        );
        assert_eq!(
            kinds(&lex("a /* c */ b")),
            vec![
                TokenKind::Ident,
                TokenKind::Comment,
                TokenKind::Ident,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn music_line_snapshot() {
        let lexed = lex("time 4/4\n3:2 ~ 3:2 5:0_8");
        assert!(lexed.diagnostics.is_empty());
        insta::assert_debug_snapshot!(lexed.tokens);
    }

    #[test]
    fn delimiters_comma_and_eq() {
        let lexed = lex("[ ] { } ( ) , =");
        assert_eq!(
            kinds(&lexed),
            vec![
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Comma,
                TokenKind::Eq,
                TokenKind::Eof,
            ]
        );
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn dot_family_is_maximal_munch() {
        assert_eq!(
            kinds(&lex(". .. ...")),
            vec![
                TokenKind::Dot,
                TokenKind::DotDot,
                TokenKind::Ellipsis,
                TokenKind::Eof,
            ]
        );
        // `....` is `...` then `.`
        assert_eq!(
            kinds(&lex("....")),
            vec![TokenKind::Ellipsis, TokenKind::Dot, TokenKind::Eof]
        );
    }

    #[test]
    fn mark_index_and_spread_lower_to_dot_tokens() {
        // Mark: `3:0.t` → … Dot Ident; the parser classifies mark vs index.
        assert_eq!(
            kinds(&lex("3:0.t")),
            vec![
                TokenKind::Int,
                TokenKind::Colon,
                TokenKind::Int,
                TokenKind::Dot,
                TokenKind::Ident,
                TokenKind::Eof,
            ]
        );
        // Index: `phrase.0` → Ident Dot Int.
        assert_eq!(
            kinds(&lex("phrase.0")),
            vec![
                TokenKind::Ident,
                TokenKind::Dot,
                TokenKind::Int,
                TokenKind::Eof
            ]
        );
        // Spread: `...g_chord` → Ellipsis Ident.
        assert_eq!(
            kinds(&lex("...g_chord")),
            vec![TokenKind::Ellipsis, TokenKind::Ident, TokenKind::Eof]
        );
    }

    /// The canonical example program (`examples/showcase.ctab` at the repo
    /// root) must now lex with no `Error` tokens and no diagnostics.
    const SHOWCASE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/showcase.ctab"
    ));

    #[test]
    fn showcase_lexes_cleanly() {
        let lexed = lex(SHOWCASE);
        assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);
        assert!(
            !lexed.tokens.iter().any(|t| t.kind == TokenKind::Error),
            "unexpected Error tokens"
        );
        assert_eq!(lexed.tokens.last().unwrap().kind, TokenKind::Eof);
        insta::assert_snapshot!(compact(&lexed));
    }
}
