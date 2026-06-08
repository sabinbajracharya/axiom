//! Type-check errors and unified diagnostics.
//!
//! `TypeDiagnostic` captures type-check specific errors. The top-level `Diagnostic`
//! enum wraps both HIR-level diagnostics (from lowering/resolution) and type-check
//! errors into one unified vector — so downstream consumers never need to know
//! which phase produced an error.
//!
//! Each `TypeDiagnostic` variant corresponds to a specific type-error scenario.
//! The coverage invariant (`check_all`) verifies that every `Ty::Error` in the
//! TypeMap has a matching diagnostic, and vice versa.

use lexer::Span;
use std::fmt;

/// A unified diagnostic that folds all pipeline phases into one vector.
/// Consumers iterate one vec, check one `kind()`, without knowing which phase
/// produced the error.
#[derive(Debug, Clone)]
pub enum Diagnostic {
    /// HIR-level diagnostic (lowering, resolution, annotation validation).
    Hir(hir::HirDiagnostic),
    /// Type-check diagnostic.
    Type(TypeDiagnostic),
}

impl Diagnostic {
    pub fn span(&self) -> Span {
        match self {
            Diagnostic::Hir(d) => d.span(),
            Diagnostic::Type(d) => d.span(),
        }
    }

    pub fn render(&self, source: &str) -> String {
        match self {
            Diagnostic::Hir(d) => d.render(source),
            Diagnostic::Type(d) => d.render(source),
        }
    }

    /// Returns a short label string usable in tests and diagnostic filtering.
    pub fn kind(&self) -> &'static str {
        match self {
            Diagnostic::Hir(d) => d.kind(),
            Diagnostic::Type(d) => d.kind(),
        }
    }
}

impl<'a> From<&'a Diagnostic> for &'a str {
    fn from(d: &'a Diagnostic) -> &'a str {
        d.kind()
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Diagnostic::Hir(d) => write!(f, "{d}"),
            Diagnostic::Type(d) => write!(f, "{d}"),
        }
    }
}

/// A type-check diagnostic: what went wrong during type checking and where.
#[derive(Debug, Clone, thiserror::Error)]
pub enum TypeDiagnostic {
    #[error("type mismatch: expected `{expected}`, found `{found}`")]
    TypeMismatch {
        expected: String,
        found: String,
        span: Span,
    },

    #[error("undefined type: `{name}`")]
    UndefinedType { name: String, span: Span },

    #[error("unknown field `{field}` on type `{ty}`")]
    UnknownField {
        field: String,
        ty: String,
        span: Span,
    },

    #[error("unknown variant `{variant}` on enum `{name}`")]
    UnknownVariant {
        variant: String,
        name: String,
        span: Span,
    },

    #[error("call arity mismatch: `{name}` expects {expected} argument(s), found {found}")]
    CallArityMismatch {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("struct `{name}` expects {expected} field(s), found {found}")]
    StructFieldCountMismatch {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("struct `{name}` missing field `{field}`")]
    StructMissingField {
        name: String,
        field: String,
        span: Span,
    },

    #[error("struct `{name}` has unknown field `{field}`")]
    StructUnknownField {
        name: String,
        field: String,
        span: Span,
    },

    #[error("non-exhaustive match: patterns do not cover all possible values")]
    NonExhaustiveMatch { missing: Vec<String>, span: Span },

    #[error("match arms have inconsistent types: expected `{expected}`, arm has `{found}`")]
    MatchArmTypeMismatch {
        expected: String,
        found: String,
        arm_index: usize,
        span: Span,
    },

    #[error("if branches have inconsistent types: expected `{expected}`, else has `{found}`")]
    IfBranchMismatch {
        expected: String,
        found: String,
        span: Span,
    },

    #[error("loop body must produce Unit")]
    LoopBodyNotUnit { found: String, span: Span },

    #[error("condition must be Bool, found `{found}`")]
    ConditionNotBool { found: String, span: Span },

    #[error("cannot call `{name}`: not a function")]
    NotCallable {
        name: String,
        found: String,
        span: Span,
    },

    #[error("binary operator `{op}` cannot be applied to `{left}` and `{right}`")]
    BinOpMismatch {
        op: String,
        left: String,
        right: String,
        span: Span,
    },

    #[error("unary operator `{op}` cannot be applied to `{operand}`")]
    UnaryOpMismatch {
        op: String,
        operand: String,
        span: Span,
    },

    #[error("cannot assign to immutable binding `{name}`")]
    AssignToImmutable { name: String, span: Span },

    #[error("return type mismatch: expected `{expected}`, body produces `{found}`")]
    ReturnTypeMismatch {
        expected: String,
        found: String,
        span: Span,
    },

    #[error("if without else must produce Unit, found `{found}`")]
    IfWithoutElseNotUnit { found: String, span: Span },

    #[error("`{feature}` is not yet supported in type checking")]
    NotYetSupported { feature: String, span: Span },

    #[error("break type mismatch: expected `{expected}`, found `{found}`")]
    BreakTypeMismatch {
        expected: String,
        found: String,
        span: Span,
    },

    #[error(
        "missing required method `{method}` in impl of trait `{trait_name}` for `{type_name}`"
    )]
    MissingTraitMethod {
        trait_name: String,
        type_name: String,
        method: String,
        span: Span,
    },

    #[error("unknown method `{method}` on type `{ty}`")]
    UnknownMethod {
        method: String,
        ty: String,
        span: Span,
    },

    #[error("trait `{name}` not found")]
    TraitNotFound { name: String, span: Span },

    #[error("cannot assign to `{ty}[…]`: type `{ty}` has no writable subscript")]
    NoWritableSubscript { ty: String, span: Span },

    #[error("type `{name}` not found for impl")]
    TypeNotFoundForImpl { name: String, span: Span },

    #[error(
        "duplicate subscript with the same index-parameter count \
         in impl for `{type_name}`; only one read and one write \
         subscript is allowed per index shape"
    )]
    DuplicateSubscript {
        type_name: String,
        index_param_count: usize,
        kind: String,
        span: Span,
    },

    #[error(
        "type `{type_name}` does not satisfy bound `{bound}` \
         required by type parameter `{param}`"
    )]
    UnsatisfiedBound {
        type_name: String,
        bound: String,
        param: String,
        span: Span,
    },
}

// ── Rendering ─────────────────────────────────────────────────────────────────

impl TypeDiagnostic {
    pub fn span(&self) -> Span {
        match self {
            TypeDiagnostic::TypeMismatch { span, .. }
            | TypeDiagnostic::UndefinedType { span, .. }
            | TypeDiagnostic::UnknownField { span, .. }
            | TypeDiagnostic::UnknownVariant { span, .. }
            | TypeDiagnostic::CallArityMismatch { span, .. }
            | TypeDiagnostic::StructFieldCountMismatch { span, .. }
            | TypeDiagnostic::StructMissingField { span, .. }
            | TypeDiagnostic::StructUnknownField { span, .. }
            | TypeDiagnostic::NonExhaustiveMatch { span, .. }
            | TypeDiagnostic::MatchArmTypeMismatch { span, .. }
            | TypeDiagnostic::IfBranchMismatch { span, .. }
            | TypeDiagnostic::LoopBodyNotUnit { span, .. }
            | TypeDiagnostic::ConditionNotBool { span, .. }
            | TypeDiagnostic::NotCallable { span, .. }
            | TypeDiagnostic::BinOpMismatch { span, .. }
            | TypeDiagnostic::UnaryOpMismatch { span, .. }
            | TypeDiagnostic::AssignToImmutable { span, .. }
            | TypeDiagnostic::ReturnTypeMismatch { span, .. }
            | TypeDiagnostic::IfWithoutElseNotUnit { span, .. }
            | TypeDiagnostic::NotYetSupported { span, .. }
            | TypeDiagnostic::BreakTypeMismatch { span, .. }
            | TypeDiagnostic::MissingTraitMethod { span, .. }
            | TypeDiagnostic::UnknownMethod { span, .. }
            | TypeDiagnostic::TraitNotFound { span, .. }
            | TypeDiagnostic::TypeNotFoundForImpl { span, .. }
            | TypeDiagnostic::NoWritableSubscript { span, .. }
            | TypeDiagnostic::DuplicateSubscript { span, .. }
            | TypeDiagnostic::UnsatisfiedBound { span, .. } => *span,
        }
    }

    /// Render to `line:col: message` using the source text for line lookup.
    pub fn render(&self, source: &str) -> String {
        use lexer::LineMap;
        let map = LineMap::new(source);
        let (line, col) = map.locate(source, self.span().lo);
        format!("{line}:{col}: {self}")
    }
}

impl TypeDiagnostic {
    /// A human-readable short label for the diagnostic kind, useful for
    /// categorization in tests. Not used in rendered output.
    pub fn kind(&self) -> &'static str {
        match self {
            TypeDiagnostic::TypeMismatch { .. } => "type_mismatch",
            TypeDiagnostic::UndefinedType { .. } => "undefined_type",
            TypeDiagnostic::UnknownField { .. } => "unknown_field",
            TypeDiagnostic::UnknownVariant { .. } => "unknown_variant",
            TypeDiagnostic::CallArityMismatch { .. } => "call_arity_mismatch",
            TypeDiagnostic::StructFieldCountMismatch { .. } => "struct_field_count_mismatch",
            TypeDiagnostic::StructMissingField { .. } => "struct_missing_field",
            TypeDiagnostic::StructUnknownField { .. } => "struct_unknown_field",
            TypeDiagnostic::NonExhaustiveMatch { .. } => "non_exhaustive_match",
            TypeDiagnostic::MatchArmTypeMismatch { .. } => "match_arm_type_mismatch",
            TypeDiagnostic::IfBranchMismatch { .. } => "if_branch_mismatch",
            TypeDiagnostic::LoopBodyNotUnit { .. } => "loop_body_not_unit",
            TypeDiagnostic::ConditionNotBool { .. } => "condition_not_bool",
            TypeDiagnostic::NotCallable { .. } => "not_callable",
            TypeDiagnostic::BinOpMismatch { .. } => "bin_op_mismatch",
            TypeDiagnostic::UnaryOpMismatch { .. } => "unary_op_mismatch",
            TypeDiagnostic::AssignToImmutable { .. } => "assign_to_immutable",
            TypeDiagnostic::ReturnTypeMismatch { .. } => "return_type_mismatch",
            TypeDiagnostic::IfWithoutElseNotUnit { .. } => "if_without_else_not_unit",
            TypeDiagnostic::NotYetSupported { .. } => "not_yet_supported",
            TypeDiagnostic::BreakTypeMismatch { .. } => "break_type_mismatch",
            TypeDiagnostic::MissingTraitMethod { .. } => "missing_trait_method",
            TypeDiagnostic::UnknownMethod { .. } => "unknown_method",
            TypeDiagnostic::TraitNotFound { .. } => "trait_not_found",
            TypeDiagnostic::TypeNotFoundForImpl { .. } => "type_not_found_for_impl",
            TypeDiagnostic::NoWritableSubscript { .. } => "no_writable_subscript",
            TypeDiagnostic::DuplicateSubscript { .. } => "duplicate_subscript",
            TypeDiagnostic::UnsatisfiedBound { .. } => "unsatisfied_bound",
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span { lo: 0, hi: 0 }
    }

    #[test]
    fn test_type_mismatch_message() {
        let diag = TypeDiagnostic::TypeMismatch {
            expected: "Int".to_string(),
            found: "Float".to_string(),
            span: span(),
        };
        assert_eq!(
            diag.to_string(),
            "type mismatch: expected `Int`, found `Float`"
        );
    }

    #[test]
    fn test_undefined_type_message() {
        let diag = TypeDiagnostic::UndefinedType {
            name: "Foo".to_string(),
            span: span(),
        };
        assert_eq!(diag.to_string(), "undefined type: `Foo`");
    }

    #[test]
    fn test_non_exhaustive_match_message() {
        let diag = TypeDiagnostic::NonExhaustiveMatch {
            missing: vec!["Empty".to_string()],
            span: span(),
        };
        assert!(diag.to_string().contains("non-exhaustive match"));
    }

    #[test]
    fn test_call_arity_mismatch_message() {
        let diag = TypeDiagnostic::CallArityMismatch {
            name: "add".to_string(),
            expected: 2,
            found: 1,
            span: span(),
        };
        assert!(diag.to_string().contains("add"));
        assert!(diag.to_string().contains("2"));
        assert!(diag.to_string().contains("1"));
    }

    #[test]
    fn test_assign_to_immutable_message() {
        let diag = TypeDiagnostic::AssignToImmutable {
            name: "x".to_string(),
            span: span(),
        };
        assert!(diag.to_string().contains("x"));
        assert!(diag.to_string().contains("immutable"));
    }

    #[test]
    fn test_kind_labels_match_variants() {
        let s = Span { lo: 0, hi: 0 };
        assert_eq!(
            TypeDiagnostic::TypeMismatch {
                expected: "A".into(),
                found: "B".into(),
                span: s,
            }
            .kind(),
            "type_mismatch"
        );
        assert_eq!(
            TypeDiagnostic::UndefinedType {
                name: "X".into(),
                span: s,
            }
            .kind(),
            "undefined_type"
        );
        assert_eq!(
            TypeDiagnostic::NonExhaustiveMatch {
                missing: vec![],
                span: s,
            }
            .kind(),
            "non_exhaustive_match"
        );
        assert_eq!(
            TypeDiagnostic::NotYetSupported {
                feature: "X".into(),
                span: s,
            }
            .kind(),
            "not_yet_supported"
        );
    }

    #[test]
    fn test_render_with_source() {
        let source = "fn main() { val x: Int = 3.14 }";
        let diag = TypeDiagnostic::TypeMismatch {
            expected: "Int".to_string(),
            found: "Float".to_string(),
            span: Span { lo: 10, hi: 12 },
        };
        let rendered = diag.render(source);
        assert!(rendered.contains(": "));
        assert!(rendered.contains("type mismatch"));
    }
}
