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

impl Diagnostic {
    pub fn new(severity: Severity, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity,
            span,
            message: message.into(),
            help: None,
        }
    }

    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, span, message)
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, span, message)
    }

    pub fn info(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Info, span, message)
    }

    /// Attach a `help` hint, builder-style.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_set_severity_and_default_no_help() {
        let d = Diagnostic::error(Span::new(0, 3), "boom");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.span, Span::new(0, 3));
        assert_eq!(d.message, "boom");
        assert_eq!(d.help, None);

        assert_eq!(
            Diagnostic::warning(Span::point(2), "w").severity,
            Severity::Warning
        );
        assert_eq!(
            Diagnostic::info(Span::point(2), "i").severity,
            Severity::Info
        );
    }

    #[test]
    fn with_help_attaches_hint() {
        let d = Diagnostic::error(Span::new(1, 2), "nope").with_help("try this");
        assert_eq!(d.help.as_deref(), Some("try this"));
    }
}
