//! The pure, UI-free core of cadtab: the `source text → render tree` pipeline.

pub mod ast;
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
use crate::layout::LayoutConfig;
use crate::render::{LayoutMeta, MeasureBox, Primitive, Rect, RenderTree, System, TextRole};
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

/// Compile source text into a render tree plus diagnostics and tokens.
///
/// Stub: ignores the source and returns a fixed tree of one string line with a
/// single `0` fret number, in logical coordinates (1 unit = string spacing).
/// The real pipeline replaces this incrementally.
pub fn compile(_source: &str, _config: LayoutConfig) -> CompileResult {
    const W: f32 = 12.0;
    const H: f32 = 4.0;
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: W,
        h: H,
    };
    let line = Primitive::Line {
        x1: 0.0,
        y1: 2.0,
        x2: W,
        y2: 2.0,
        weight: 0.1,
    };
    let fret = Primitive::Text {
        x: 1.0,
        y: 2.0,
        content: "0".to_string(),
        role: TextRole::FretNumber,
        span: None,
    };
    let measure = MeasureBox {
        bounds,
        prims: vec![line, fret],
        span: None,
    };
    let system = System {
        bounds,
        prims: vec![],
        measures: vec![measure],
    };
    CompileResult {
        render_tree: RenderTree {
            meta: LayoutMeta {
                width: W,
                height: H,
            },
            header: vec![],
            systems: vec![system],
        },
        diagnostics: vec![],
        tokens: vec![],
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

    #[test]
    fn stub_returns_one_line_and_one_fret() {
        let result = compile("3:0", LayoutConfig { width: 800.0 });
        assert!(result.diagnostics.is_empty());
        assert!(result.tokens.is_empty());

        let systems = &result.render_tree.systems;
        assert_eq!(systems.len(), 1);
        let prims = &systems[0].measures[0].prims;
        assert_eq!(prims.len(), 2);
        assert!(matches!(prims[0], Primitive::Line { .. }));
        assert!(matches!(
            &prims[1],
            Primitive::Text { content, role: TextRole::FretNumber, .. } if content == "0"
        ));
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
        round_trip(&compile("3:0", LayoutConfig { width: 640.0 }));
    }

    #[test]
    fn compile_stub_wire_format() {
        let result = compile("3:0", LayoutConfig { width: 800.0 });
        insta::assert_snapshot!(serde_json::to_string_pretty(&result).unwrap());
    }
}
