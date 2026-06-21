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
                // Delimiters, `.`/`...`, `,`, `=`: not yet scanned.
                _ => self.unknown_char(),
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

    /// A run of ASCII digits.
    fn number(&mut self) -> TokenKind {
        while matches!(self.peek(), Some(b) if b.is_ascii_digit()) {
            self.pos += 1;
        }
        TokenKind::Int
    }

    /// An identifier starting with an ASCII letter, then letters/digits/`_`.
    /// Recognized keywords lower to [`TokenKind::Keyword`].
    fn ident(&mut self, start: u32) -> TokenKind {
        while matches!(self.peek(), Some(b) if b == b'_' || b.is_ascii_alphanumeric()) {
            self.pos += 1;
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

    /// Consume one whole UTF-8 char as an `Error` token; advancing by a full
    /// char keeps spans on char boundaries.
    fn unknown_char(&mut self) -> TokenKind {
        let len = self.src[self.pos..]
            .chars()
            .next()
            .map_or(1, char::len_utf8);
        self.pos += len;
        TokenKind::Error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lexed: &Lexed) -> Vec<TokenKind> {
        lexed.tokens.iter().map(|t| t.kind).collect()
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
        let lexed = lex("title \"Cripple Creek\"\ncomposer \"trad.\"\ntempo 130");
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
}
