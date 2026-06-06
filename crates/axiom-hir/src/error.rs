//! HIR diagnostics. A `HirDiagnostic` is a message plus a byte span; rendering
//! to a human-facing string uses the lexer's `LineMap` + `Span`, mirroring
//! `ParseError::render`.

use axiom_lexer::Span;

/// A single HIR diagnostic: what went wrong and where.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum HirDiagnostic {
    #[error("unresolved name: `{name}`")]
    UnresolvedName { name: String, span: Span },
    #[error("duplicate definition: `{name}`")]
    DuplicateDefinition { name: String, span: Span },
    #[error("{kind} `{name}` expects {expected} argument(s), found {found}")]
    ArityMismatch {
        kind: String,
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },
    #[error("{feature} is not yet supported in this version")]
    NotYetSupported { feature: String, span: Span },
    #[error("`{name}` is private in module `{module}`")]
    PrivateImport {
        name: String,
        module: String,
        span: Span,
    },
    #[error("required lang item `{key}` was not found in the standard library")]
    MissingLangItem { key: String, span: Span },
    #[error("lang item `{key}` is bound more than once")]
    DuplicateLangItem { key: String, span: Span },
    #[error("unknown lang item `{key}`: no compiler consumer for this `@lang` tag")]
    OrphanLangItem { key: String, span: Span },
    #[error("`@lang` attributes are only allowed in the standard library (found `{key}`)")]
    LangItemOutsideStdlib { key: String, span: Span },
}

impl HirDiagnostic {
    pub fn span(&self) -> Span {
        match self {
            HirDiagnostic::UnresolvedName { span, .. }
            | HirDiagnostic::DuplicateDefinition { span, .. }
            | HirDiagnostic::ArityMismatch { span, .. }
            | HirDiagnostic::NotYetSupported { span, .. }
            | HirDiagnostic::PrivateImport { span, .. }
            | HirDiagnostic::MissingLangItem { span, .. }
            | HirDiagnostic::DuplicateLangItem { span, .. }
            | HirDiagnostic::OrphanLangItem { span, .. }
            | HirDiagnostic::LangItemOutsideStdlib { span, .. } => *span,
        }
    }

    pub fn render(&self, source: &str) -> String {
        use axiom_lexer::LineMap;
        let map = LineMap::new(source);
        let (line, col) = map.locate(source, self.span().lo);
        format!("{line}:{col}: {}", self)
    }
}
