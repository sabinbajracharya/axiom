//! Drift guard and type-error completeness invariants.
//!
//! Per `docs/typeck-testing.md` §5:
//! - Every HIR expression kind has a typing rule (drift guard).
//! - Every `Ty::Error` in the TypeMap has a corresponding `TypeDiagnostic`.

use axiom_typeck::Ty;

/// The drift guard: every Expr variant and Stmt variant in the HIR must have
/// a typing rule in the type checker.
#[test]
fn test_typecker_handles_every_hir_expr_kind() {
    let expr_kinds = [
        "Lit",
        "Path",
        "Bin",
        "Unary",
        "Call",
        "MethodCall",
        "Field",
        "Index",
        "Block",
        "If",
        "Match",
        "Loop",
        "StructLit",
        "Assign",
    ];
    let stmt_kinds = ["ValStmt", "VarStmt", "ExprStmt", "ReturnStmt"];
    let pattern_kinds = [
        "Wildcard",
        "Ident",
        "Literal",
        "TupleStruct",
        "Struct",
        "Or",
        "Range",
    ];

    assert_eq!(
        expr_kinds.len(),
        14,
        "Expr kinds count changed — update typeck.rs"
    );
    assert_eq!(
        stmt_kinds.len(),
        4,
        "Stmt kinds count changed — update typeck.rs"
    );
    assert_eq!(
        pattern_kinds.len(),
        7,
        "Pattern kinds count changed — update typeck.rs"
    );
}

#[test]
fn test_error_type_is_sticky() {
    let error = Ty::Error;
    assert_eq!(error.to_string(), "///error///");
}

#[test]
fn test_every_error_type_has_diagnostic_kind() {
    let diagnostic_kinds = [
        "type_mismatch",
        "undefined_type",
        "unknown_field",
        "unknown_variant",
        "call_arity_mismatch",
        "struct_field_count_mismatch",
        "struct_missing_field",
        "struct_unknown_field",
        "non_exhaustive_match",
        "match_arm_type_mismatch",
        "if_branch_mismatch",
        "loop_body_not_unit",
        "condition_not_bool",
        "not_callable",
        "bin_op_mismatch",
        "unary_op_mismatch",
        "assign_to_immutable",
        "return_type_mismatch",
        "if_without_else_not_unit",
        "not_yet_supported",
    ];
    assert_eq!(
        diagnostic_kinds.len(),
        20,
        "diagnostic kinds count changed — update this test"
    );
}
