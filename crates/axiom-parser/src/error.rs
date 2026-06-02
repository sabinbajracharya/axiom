//! Parse-stage diagnostics. A `ParseError` is a message plus the byte span it
//! refers to; rendering to a human-facing string lives in `render` (used by the
//! diagnostics snapshot tests). The parser is **total** — it collects these in
//! `ParseResult::errors` and never fails or panics (`docs/parser-testing.md` §5).

use axiom_lexer::{LineMap, Span};

/// A single parse diagnostic: what went wrong and where.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl ParseError {
    pub fn new(message: impl Into<String>, span: Span) -> ParseError {
        ParseError {
            message: message.into(),
            span,
        }
    }

    /// Render as `line:col: message` against the source (for diagnostic
    /// snapshots), mirroring the lexer's diagnostic format.
    pub fn render(&self, source: &str) -> String {
        let map = LineMap::new(source);
        let (line, col) = map.locate(source, self.span.lo);
        format!("{line}:{col}: {}", self.message)
    }
}
