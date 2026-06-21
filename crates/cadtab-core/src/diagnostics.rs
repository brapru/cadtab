use serde::{Deserialize, Serialize};

use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A single problem reported against a span of source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    pub help: Option<String>,
}
