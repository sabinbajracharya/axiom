//! Integration tests for generics Phase 2: type checker unification.
//!
//! Tests that generic functions with type parameters are correctly inferred
//! at call sites, with proper error reporting for type mismatches.

use parser::ast::AstNode;
use resolver::lower;
use typecheck::Ty;

#[allow(clippy::unwrap_used)]
fn check_source(source: &str) -> typecheck::Thir {
    let result = parser::parse(source);
    let root = parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = lower(&root, source, None);
    typecheck::check(hir)
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

// ── Impl method with own type params ────────────────────────────────────────

#[test]
fn test_impl_method_with_own_type_param_no_error() {
    // An impl method declaring its own type param should not produce
    // undefined_type errors — the type param should resolve in scope.
    let thir = check_source(
        "struct Wrapper<T> { val: T }
impl<T> Wrapper<T> {
    fn map<S>(self) -> S { todo() }
}
fn main() { }",
    );
    let errors: Vec<_> = thir
        .diagnostics
        .iter()
        .filter(|d| d.kind() == "undefined_type")
        .collect();
    assert!(
        errors.is_empty(),
        "S should resolve as method type param, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_impl_method_return_type_param_uses_correct_scope() {
    // The return type S in fn convert<S> should be in scope.
    // When resolved, it should be Ty::TypeParam, not Ty::Error.
    let thir = check_source(
        "struct Wrapper<T> { val: T }
impl<T> Wrapper<T> {
    fn convert<S>(self) -> S { todo() }
}
fn main() { }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected clean typecheck, got: {:?}",
        thir.diagnostics
    );
}

// ── THIR dump ───────────────────────────────────────────────────────────────

#[test]
fn generic_fn_type_param_in_thir() {
    let thir = check_source("fn id<T>(let x: T) -> T { x }");
    let dump = typecheck::serialize(&thir, None);
    // The THIR dump should contain the type parameter T.
    assert!(dump.contains('T'), "expected T in THIR dump:\n{dump}");
}

// ── Match/if branch unification of generic return types ────────────────────
//
// When a generic enum constructor is called in one branch and the other
// branch produces the same enum directly (e.g. `Some(x)` vs `None`), the
// resulting types carry different `TypeParamId` def_ids — the first is
// bound to the function's scope, the second retains the enum's own.
// PartialEq sees them as different, but unify should accept them.

#[test]
fn generic_if_branches_unify_enum() {
    let thir = check_source(
        "enum Wrapper<T> { Has(T), Empty }
fn test<T>(x: T) -> Wrapper<T> {
    if true {
        Has(x)
    } else {
        Empty
    }
}",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn generic_match_arms_unify_enum() {
    let thir = check_source(
        "enum Wrapper<T> { Has(T), Empty }
fn test<T>(x: Wrapper<T>) -> Wrapper<T> {
    match x {
        Has(v) => Has(v),
        Empty => Empty,
    }
}",
    );
    let has_mismatch = thir
        .diagnostics
        .iter()
        .any(|d| d.kind() == "match_arm_type_mismatch");
    assert!(
        !has_mismatch,
        "expected no match_arm_type_mismatch, got: {:?}",
        thir.diagnostics
    );
}
