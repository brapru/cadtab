use serde::{Deserialize, Serialize};

use crate::span::Span;

/// Highlight classification for a lexed token. The lexer is the single source
/// for both syntax highlighting and diagnostics; this is the class the editor
/// renders as a decoration.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub class: TokenClass,
    pub span: Span,
}
