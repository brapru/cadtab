use serde::{Deserialize, Serialize};

use crate::span::Span;

/// Highlight classification for a lexed token. The lexer is the single source
/// for both syntax highlighting and diagnostics; this is the class the editor
/// renders as a decoration. Crosses the wire as part of [`Token`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenClass {
    Keyword,
    Number,
    String,
    Comment,
    Ident,
    Operator,
    Punctuation,
}

/// The wire token sent to the frontend for highlighting: a coarse class + span.
/// The precise lexical [`TokenKind`] stays Rust-side for the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub class: TokenClass,
    pub span: Span,
}

/// Reserved words recognized from identifiers. `r` (rest) is deliberately *not*
/// here — it is lexed as an `Ident` and recognized as a rest only in event
/// position (parser's job).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Keyword {
    Title,
    Composer,
    Tempo,
    Instrument,
    Tuning,
    Capo,
    Import,
    BarNumbers,
    Score,
    Time,
    Default,
    Pickup,
    Repeat,
    Ending,
    Loop,
    Measure,
    Section,
    Def,
    Let,
}

impl Keyword {
    /// Every keyword, in declaration order — the single list driving the
    /// editor's keyword completions (D46), so the JS side keeps no copy.
    pub const ALL: &'static [Keyword] = &[
        Keyword::Title,
        Keyword::Composer,
        Keyword::Tempo,
        Keyword::Instrument,
        Keyword::Tuning,
        Keyword::Capo,
        Keyword::Import,
        Keyword::BarNumbers,
        Keyword::Score,
        Keyword::Time,
        Keyword::Default,
        Keyword::Pickup,
        Keyword::Repeat,
        Keyword::Ending,
        Keyword::Loop,
        Keyword::Measure,
        Keyword::Section,
        Keyword::Def,
        Keyword::Let,
    ];

    /// The source spelling of this keyword.
    pub fn as_str(self) -> &'static str {
        use Keyword::*;
        match self {
            Title => "title",
            Composer => "composer",
            Tempo => "tempo",
            Instrument => "instrument",
            Tuning => "tuning",
            Capo => "capo",
            Import => "import",
            BarNumbers => "barnumbers",
            Score => "score",
            Time => "time",
            Default => "default",
            Pickup => "pickup",
            Repeat => "repeat",
            Ending => "ending",
            Loop => "loop",
            Measure => "measure",
            Section => "section",
            Def => "def",
            Let => "let",
        }
    }

    /// Map an identifier spelling to a keyword, if it is one.
    pub fn from_ident(text: &str) -> Option<Keyword> {
        use Keyword::*;
        Some(match text {
            "title" => Title,
            "composer" => Composer,
            "tempo" => Tempo,
            "instrument" => Instrument,
            "tuning" => Tuning,
            "capo" => Capo,
            "import" => Import,
            "barnumbers" => BarNumbers,
            "score" => Score,
            "time" => Time,
            "default" => Default,
            "pickup" => Pickup,
            "repeat" => Repeat,
            "ending" => Ending,
            "loop" => Loop,
            "measure" => Measure,
            "section" => Section,
            "def" => Def,
            "let" => Let,
            _ => return None,
        })
    }
}

/// The precise lexical kind of a token. Atomic: the lexer emits single
/// `.` / `_` / `:` / `~` tokens; the parser assembles marks (`.t`), indices
/// (`.0`), and durations (`_8.`). Values (int/ident/string text) are recovered
/// by the parser from `source[span]`, so this stays `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenKind {
    // Literals & names
    Int,
    Str,
    Ident,
    Keyword(Keyword),

    // Music / operator punctuation
    Colon,      // `:`  note string:fret separator
    Underscore, // `_`  duration-suffix lead
    Tilde,      // `~`  tie
    Dot,        // `.`  mark / index / bare dot (parser disambiguates)
    DotDot,     // `..` reserved (no current use)
    Ellipsis,   // `...` spread
    Slash,      // `/`  time-signature separator
    Eq,         // `=`  let binding
    Comma,      // `,`

    // Delimiters
    LBracket, // `[`
    RBracket, // `]`
    LBrace,   // `{`
    RBrace,   // `}`
    LParen,   // `(`
    RParen,   // `)`

    // Trivia & control
    Comment, // `//` line or `/* */` block
    Error,   // an unrecognized / malformed run
    Eof,     // end of input sentinel
}

impl TokenKind {
    /// Highlight class for this kind, or `None` for kinds the editor does not
    /// decorate (`Eof`, `Error`).
    pub fn class(&self) -> Option<TokenClass> {
        use TokenKind::*;
        Some(match self {
            Int => TokenClass::Number,
            Str => TokenClass::String,
            Ident => TokenClass::Ident,
            Keyword(_) => TokenClass::Keyword,
            Comment => TokenClass::Comment,
            Colon | Underscore | Tilde | Dot | DotDot | Ellipsis | Slash | Eq => {
                TokenClass::Operator
            }
            Comma | LBracket | RBracket | LBrace | RBrace | LParen | RParen => {
                TokenClass::Punctuation
            }
            Eof | Error => return None,
        })
    }
}

/// A token as produced by the lexer: a precise [`TokenKind`] plus its source
/// span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LexToken {
    pub kind: TokenKind,
    pub span: Span,
}

impl LexToken {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// The wire highlight token for this lex token, or `None` if not decorated.
    pub fn highlight(&self) -> Option<Token> {
        self.kind.class().map(|class| Token {
            class,
            span: self.span,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_lookup_round_trips_spellings() {
        assert_eq!(Keyword::from_ident("score"), Some(Keyword::Score));
        assert_eq!(Keyword::from_ident("loop"), Some(Keyword::Loop));
        assert_eq!(Keyword::from_ident("ending"), Some(Keyword::Ending));
        // contextual / not keywords
        assert_eq!(Keyword::from_ident("r"), None);
        assert_eq!(Keyword::from_ident("hammer"), None);
        assert_eq!(Keyword::from_ident("Score"), None); // case-sensitive
    }

    #[test]
    fn all_keywords_are_listed_once_and_spell_themselves() {
        // ALL is the single completion list; every entry must round-trip through
        // its spelling and appear exactly once.
        let mut spellings: Vec<&str> = Keyword::ALL
            .iter()
            .map(|&kw| {
                assert_eq!(Keyword::from_ident(kw.as_str()), Some(kw));
                kw.as_str()
            })
            .collect();
        let listed = spellings.len();
        spellings.sort_unstable();
        spellings.dedup();
        assert_eq!(spellings.len(), listed, "ALL has a duplicate keyword");
        assert_eq!(listed, 19);
    }

    #[test]
    fn keyword_as_str_round_trips() {
        for kw in [
            Keyword::Score,
            Keyword::Loop,
            Keyword::Def,
            Keyword::Ending,
            Keyword::Section,
        ] {
            assert_eq!(Keyword::from_ident(kw.as_str()), Some(kw));
        }
    }

    #[test]
    fn classes_map_for_highlighting() {
        assert_eq!(TokenKind::Int.class(), Some(TokenClass::Number));
        assert_eq!(TokenKind::Str.class(), Some(TokenClass::String));
        assert_eq!(TokenKind::Ident.class(), Some(TokenClass::Ident));
        assert_eq!(
            TokenKind::Keyword(Keyword::Score).class(),
            Some(TokenClass::Keyword)
        );
        assert_eq!(TokenKind::Comment.class(), Some(TokenClass::Comment));
        assert_eq!(TokenKind::Tilde.class(), Some(TokenClass::Operator));
        assert_eq!(TokenKind::LBracket.class(), Some(TokenClass::Punctuation));
        // not decorated
        assert_eq!(TokenKind::Eof.class(), None);
        assert_eq!(TokenKind::Error.class(), None);
    }

    #[test]
    fn highlight_drops_undecorated_kinds() {
        let span = Span::new(0, 1);
        assert_eq!(
            LexToken::new(TokenKind::Int, span).highlight(),
            Some(Token {
                class: TokenClass::Number,
                span
            })
        );
        assert_eq!(LexToken::new(TokenKind::Eof, span).highlight(), None);
        assert_eq!(LexToken::new(TokenKind::Error, span).highlight(), None);
    }
}
