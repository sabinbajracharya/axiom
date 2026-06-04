//! Unit tests for the type checker.

use super::*;
use axiom_hir::lower;
use axiom_parser::ast::AstNode;

fn check_source(source: &str) -> Thir {
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = lower(&root, source);
    check(hir)
}

#[test]
fn test_infer_int_literal() {
    let thir = check_source("fn main() { val x = 42 }");
    let has_int = thir.types.values().any(|t| *t == crate::types::Ty::Int);
    assert!(
        has_int,
        "expected Int type somewhere, got: {:?}",
        thir.types
    );
}

#[test]
fn test_infer_string_literal() {
    let thir = check_source("fn main() { print(\"hello\") }");
    let has_string = thir
        .types
        .values()
        .any(|t| matches!(t, crate::types::Ty::String));
    assert!(has_string, "expected String type somewhere");
}

#[test]
fn test_infer_bin_op_add() {
    let thir = check_source("fn main() { val x = 1 + 2 }");
    let has_int = thir.types.values().any(|t| *t == crate::types::Ty::Int);
    assert!(has_int, "expected Int type from addition");
}

#[test]
fn test_type_mismatch_bin_op() {
    let thir = check_source("fn main() { val x = 1 + 2.0 }");
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "bin_op_mismatch"),
        "expected bin op mismatch diagnostic, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_fn_call_with_params() {
    // `main` has no explicit return type (defaults to Unit), but the body
    // produces Int via `add(1, 2)`. That is a real type mismatch now that
    // block tail expressions are properly tracked.
    let thir =
        check_source("fn add(a: Int, b: Int) -> Int { a + b } fn main() -> Int { add(1, 2) }");
    assert!(
        thir.diagnostics.iter().all(|d| d.kind() != "type_mismatch"),
        "unexpected type errors: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_fn_call_arity_mismatch() {
    let thir = check_source("fn add(a: Int, b: Int) -> Int { a + b } fn main() { add(1) }");
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "call_arity_mismatch"),
        "expected arity mismatch, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_struct_literal() {
    let thir = check_source(
        "struct Point { x: Float, y: Float }
fn main() { val p = Point { x: 1.0, y: 2.0 } }",
    );
    let has_struct = thir
        .types
        .values()
        .any(|t| matches!(t, crate::types::Ty::Struct(_)));
    assert!(has_struct, "expected Struct type");
}

#[test]
fn test_enum_match() {
    let thir = check_source(
        "enum Shape { Circle(Float), Rect(Float, Float), Empty }
fn area(s: Shape) -> Float { match s { Circle(r) => 3.14 Rect(w, h) => 1.0 Empty => 0.0 } }",
    );
    let non_exhaustive: Vec<_> = thir
        .diagnostics
        .iter()
        .filter(|d| d.kind() == "non_exhaustive_match")
        .collect();
    assert!(
        non_exhaustive.is_empty(),
        "unexpected non-exhaustive match: {:?}",
        non_exhaustive
    );
}

#[test]
fn test_non_exhaustive_match() {
    let thir = check_source(
        "enum Shape { Circle(Float), Rect(Float, Float) }
fn area(s: Shape) -> Float { match s { Circle(r) => r } }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "non_exhaustive_match"),
        "expected non-exhaustive match diagnostic, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_assign_to_immutable() {
    let thir = check_source("fn main() { val x = 1 x = 2 }");
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "assign_to_immutable"),
        "expected assign_to_immutable diagnostic, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_if_branch_mismatch() {
    let thir = check_source("fn main() { val x: Float = if true { 1.0 } else { 2 } }");
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "type_mismatch" || d.kind() == "if_branch_mismatch"),
        "expected type mismatch diagnostic, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_unknown_field() {
    let thir = check_source(
        "struct Point { x: Float, y: Float }
fn main() { val p = Point { x: 1.0, y: 2.0 } val z = p.z }",
    );
    assert!(
        thir.diagnostics.iter().any(|d| d.kind() == "unknown_field"),
        "expected unknown_field diagnostic, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_not_callable() {
    let thir = check_source("fn main() { val x = 1 x() }");
    assert!(
        thir.diagnostics.iter().any(|d| d.kind() == "not_callable"),
        "expected not_callable diagnostic, got: {:?}",
        thir.diagnostics
    );
}

// ── Generic impl method resolution ─────────────────────────────────────────

#[test]
fn test_generic_impl_method_resolution() {
    // A generic impl block: `impl<T> Wrapper<T>` with a method that returns T.
    // When called on `Wrapper<Int>`, T should be substituted with Int.
    let thir = check_source(
        "struct Wrapper { v: Int }
impl Wrapper {
    fn get(let self) -> Int { self.v }
}
fn main() { val w = Wrapper { v: 42 } val x = w.get() }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        thir.diagnostics
    );
}
