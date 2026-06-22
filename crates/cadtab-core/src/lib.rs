//! The pure, UI-free core of cadtab: the `source text → render tree` pipeline.

pub mod ast;
pub mod beam;
pub mod diagnostics;
pub mod eval;
pub mod instrument;
pub mod layout;
pub mod lexer;
pub mod model;
pub mod parser;
pub mod render;
pub mod resolve;
pub mod source;
pub mod span;
pub mod stdlib;
pub mod token;
pub mod types;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::eval::eval_program;
use crate::layout::{LayoutConfig, layout};
use crate::lexer::lex;
use crate::parser::parse;
use crate::render::RenderTree;
use crate::token::Token;

/// Everything a single compile produces for the frontend: the positioned render
/// tree, any diagnostics, and the classified tokens for highlighting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileResult {
    pub render_tree: RenderTree,
    pub diagnostics: Vec<Diagnostic>,
    pub tokens: Vec<Token>,
}

/// Compile source text into a render tree plus diagnostics and tokens by running
/// the full pipeline: lex (for highlight tokens) → parse → evaluate → layout.
///
/// Resilient by construction: every stage recovers and reports rather than
/// bailing, so a malformed document still yields highlight tokens, the
/// diagnostics it provoked, and a best-effort partial render tree. Lexer and
/// parser diagnostics precede evaluation diagnostics.
///
/// Highlight tokens come from a dedicated lex of the source rather than the
/// parser's stream, because the parser drops comment and error trivia that the
/// editor still wants to colour.
pub fn compile(source: &str, config: LayoutConfig) -> CompileResult {
    let tokens: Vec<Token> = lex(source)
        .tokens
        .iter()
        .filter_map(|t| t.highlight())
        .collect();

    let parsed = parse(source);
    let (score, eval_diagnostics) = eval_program(&parsed.program);
    let render_tree = layout(&score, config);

    let mut diagnostics = parsed.diagnostics;
    diagnostics.extend(eval_diagnostics);

    CompileResult {
        render_tree,
        diagnostics,
        tokens,
    }
}

/// Returns the crate version string.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{Diagnostic, Severity};
    use crate::render::{Primitive, TextRole};
    use crate::span::Span;
    use crate::token::{Token, TokenClass};
    use proptest::prelude::*;
    use serde::de::DeserializeOwned;
    use std::fmt::Debug;

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) {
        let json = serde_json::to_string(value).unwrap();
        let back: T = serde_json::from_str(&json).unwrap();
        assert_eq!(value, &back);
    }

    #[test]
    fn version_is_reported() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn version_snapshot() {
        insta::assert_snapshot!(version(), @"0.0.0");
    }

    proptest! {
        #[test]
        fn version_is_stable(repeats in 0usize..8) {
            let first = version();
            for _ in 0..repeats {
                prop_assert_eq!(version(), first);
            }
        }
    }

    /// A valid one-bar banjo program: four quarter notes fill a 4/4 measure, so
    /// it compiles cleanly with no diagnostics.
    const ONE_BAR: &str = "score { 3:0 2:0 1:0 5:0 }";

    #[test]
    fn compiles_a_valid_score_to_a_render_tree() {
        let result = compile(ONE_BAR, LayoutConfig { width: 800.0 });

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        // Header carries the string lines / metadata; the body lands in systems.
        assert!(!result.render_tree.systems.is_empty());
        let prims = &result.render_tree.systems[0].measures[0].prims;
        assert!(prims.iter().any(|p| matches!(
            p,
            Primitive::Text { content, role: TextRole::FretNumber, .. } if content == "0"
        )));
    }

    #[test]
    fn emits_highlight_tokens_for_the_source() {
        let result = compile(ONE_BAR, LayoutConfig { width: 800.0 });
        // `score` keyword, fret numbers, and `:` separators all classify.
        assert!(result.tokens.iter().any(|t| t.class == TokenClass::Keyword));
        assert!(result.tokens.iter().any(|t| t.class == TokenClass::Number));
        assert!(
            result
                .tokens
                .iter()
                .any(|t| t.class == TokenClass::Operator)
        );
    }

    #[test]
    fn comments_classify_even_though_the_parser_drops_them() {
        let result = compile("// a note\nscore { 3:0 }", LayoutConfig { width: 800.0 });
        assert!(result.tokens.iter().any(|t| t.class == TokenClass::Comment));
    }

    #[test]
    fn reports_diagnostics_but_still_renders_partially() {
        // A bare event is not a valid top-level item: parse reports, recovers,
        // and the pipeline still produces tokens and a (header-only) tree.
        let result = compile("3:0", LayoutConfig { width: 800.0 });
        assert!(!result.diagnostics.is_empty());
        assert!(!result.tokens.is_empty());
    }

    #[test]
    fn contract_types_round_trip() {
        round_trip(&Span::new(2, 7));
        round_trip(&Diagnostic {
            severity: Severity::Error,
            span: Span::new(0, 3),
            message: "boom".to_string(),
            help: Some("try this".to_string()),
        });
        round_trip(&Token {
            class: TokenClass::Number,
            span: Span::new(4, 5),
        });
        round_trip(&compile(ONE_BAR, LayoutConfig { width: 640.0 }));
    }

    #[test]
    fn compile_wire_format() {
        let result = compile("score { 3:0 }", LayoutConfig { width: 800.0 });
        insta::assert_snapshot!(serde_json::to_string_pretty(&result).unwrap());
    }
}
