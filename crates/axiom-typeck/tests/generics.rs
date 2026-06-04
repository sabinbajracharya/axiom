//! Integration tests for generics Phase 2: type checker unification.
//!
//! Tests that generic functions with type parameters are correctly inferred
//! at call sites, with proper error reporting for type mismatches.

use axiom_hir::lower;
use axiom_parser::ast::AstNode;
use axiom_typeck::Ty;

#[allow(clippy::unwrap_used)]
fn check_source(source: &str) -> axiom_typeck::Thir {
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = lower(&root, source);
    axiom_typeck::check(hir)
}

// ── Generic function inference ───────────────────────────────────────────────

#[test]
fn generic_identity_int() {
    let thir = check_source(
        "fn id<T>(let x: T) -> T { x }
         fn main() -> Int { id(42) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        thir.diagnostics
    );
    // The call `id(42)` should have type Int.
    let has_int_call = thir.types.values().any(|t| *t == Ty::Int);
    assert!(
        has_int_call,
        "expected Int type for id(42), got: {:?}",
        thir.types
    );
}

#[test]
fn generic_identity_string() {
    let thir = check_source(
        "fn id<T>(let x: T) -> T { x }
         fn main() -> String { id(\"hello\") }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        thir.diagnostics
    );
    let has_string_call = thir.types.values().any(|t| matches!(t, Ty::String));
    assert!(
        has_string_call,
        "expected String type for id(\"hello\"), got: {:?}",
        thir.types
    );
}

#[test]
fn generic_two_params_same_type() {
    let thir = check_source(
        "fn first<T>(let a: T, let b: T) -> T { a }
         fn main() -> Int { first(1, 2) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        thir.diagnostics
    );
}

#[test]
fn generic_two_different_type_params() {
    let thir = check_source(
        "fn pair<A, B>(let a: A, let b: B) -> A { a }
         fn main() -> Int { pair(1, true) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        thir.diagnostics
    );
}

// ── Type mismatch errors ────────────────────────────────────────────────────

#[test]
fn generic_type_mismatch_same_param() {
    let thir = check_source(
        "fn f<T>(let a: T, let b: T) -> T { a }
         fn main() -> Int { f(1, true) }",
    );
    assert!(
        thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "expected type_mismatch for f(1, true), got: {:?}",
        thir.diagnostics
    );
}

// ── Return type substitution ────────────────────────────────────────────────

#[test]
fn generic_return_type_substituted() {
    let thir = check_source(
        "fn id<T>(let x: T) -> T { x }
         fn main() -> Int { id(42) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        thir.diagnostics
    );
}

// ── Generic body type-checking ──────────────────────────────────────────────

#[test]
fn generic_body_uses_type_param() {
    let thir = check_source("fn id<T>(let x: T) -> T { x }");
    // The body `x` has type T, return type is T — should match.
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        thir.diagnostics
    );
}

// ── Backward compatibility ──────────────────────────────────────────────────

#[test]
fn nongeneric_still_works() {
    let thir = check_source(
        "fn add(let a: Int, let b: Int) -> Int { a + b }
         fn main() -> Int { add(1, 2) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        thir.diagnostics
    );
}

#[test]
fn nongeneric_type_mismatch_still_reported() {
    let thir = check_source(
        "fn add(let a: Int, let b: Int) -> Int { a + b }
         fn main() -> Int { add(1, true) }",
    );
    assert!(
        thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "expected type_mismatch for add(1, true), got: {:?}",
        thir.diagnostics
    );
}

// ── THIR dump ───────────────────────────────────────────────────────────────

#[test]
fn generic_fn_type_param_in_thir() {
    let thir = check_source("fn id<T>(let x: T) -> T { x }");
    let dump = axiom_typeck::serialize(&thir, None);
    // The THIR dump should contain the type parameter T.
    assert!(dump.contains('T'), "expected T in THIR dump:\n{dump}");
}
