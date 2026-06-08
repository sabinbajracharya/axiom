//! HIR diagnostics. A `HirDiagnostic` is a message plus a byte span; rendering
//! to a human-facing string uses the lexer's `LineMap` + `Span`, mirroring
//! `ParseError::render`.

use lexer::Span;

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
    #[error("`@intrinsic` attributes are only allowed in the standard library (found `{key}`)")]
    IntrinsicOutsideStdlib { key: String, span: Span },
    #[error("unknown intrinsic `{key}`: the compiler does not know how to lower it")]
    UnknownIntrinsic { key: String, span: Span },
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
            | HirDiagnostic::LangItemOutsideStdlib { span, .. }
            | HirDiagnostic::IntrinsicOutsideStdlib { span, .. }
            | HirDiagnostic::UnknownIntrinsic { span, .. } => *span,
        }
    }

    pub fn render(&self, source: &str) -> String {
        use lexer::LineMap;
        let map = LineMap::new(source);
        let (line, col) = map.locate(source, self.span().lo);
        format!("{line}:{col}: {}", self)
    }

    /// A human-readable short label for the diagnostic kind, useful for
    /// categorization in tests and diagnostic filtering.
    pub fn kind(&self) -> &'static str {
        match self {
            HirDiagnostic::UnresolvedName { .. } => "unresolved_name",
            HirDiagnostic::DuplicateDefinition { .. } => "duplicate_definition",
            HirDiagnostic::ArityMismatch { .. } => "arity_mismatch",
            HirDiagnostic::NotYetSupported { .. } => "not_yet_supported",
            HirDiagnostic::PrivateImport { .. } => "private_import",
            HirDiagnostic::MissingLangItem { .. } => "missing_lang_item",
            HirDiagnostic::DuplicateLangItem { .. } => "duplicate_lang_item",
            HirDiagnostic::OrphanLangItem { .. } => "orphan_lang_item",
            HirDiagnostic::LangItemOutsideStdlib { .. } => "lang_item_outside_stdlib",
            HirDiagnostic::IntrinsicOutsideStdlib { .. } => "intrinsic_outside_stdlib",
            HirDiagnostic::UnknownIntrinsic { .. } => "unknown_intrinsic",
        }
    }
}
