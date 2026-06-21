//! Hand-rolled lexer. Scans source into [`LexToken`]s + diagnostics without
//! bailing: unrecognized input becomes an `Error` token so the stream always
//! reaches `Eof`. Comments are emitted as trivia tokens; the parser skips them.

use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::token::{LexToken, TokenKind};

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
                // Literals, identifiers, music tokens, delimiters: not yet scanned.
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
            vec![TokenKind::Comment, TokenKind::Error, TokenKind::Eof]
        );
        assert_eq!(lexed.tokens[0].span, Span::new(0, 10)); // "/* a\n b */"
        assert_eq!(lexed.tokens[1].span, Span::new(10, 11)); // 'x' (unhandled yet)
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
}
