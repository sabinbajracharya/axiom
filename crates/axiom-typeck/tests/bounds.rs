//! Integration tests for bound checking: generics + traits integration.
//!
//! When a generic function declares trait bounds on its type parameters
//! (e.g., `fn sort<T: Ord>(items: Vec<T>)`), calling it with a concrete
//! type must verify that the type satisfies all bounds via its impl table.

use axiom_hir::lower;
use axiom_parser::ast::AstNode;
use axiom_typeck::check;

#[allow(clippy::unwrap_used)]
fn check_source(source: &str) -> axiom_typeck::Thir {
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = lower(&root, source, None);
    check(hir)
}

// ── Bound satisfied ──────────────────────────────────────────────────────────

#[test]
fn test_bound_satisfied() {
    let thir = check_source(
        "trait Ord {}
struct Foo {}
impl Ord for Foo {}
fn take_ord<T: Ord>(let x: T) -> T { x }
fn main() -> Foo { take_ord(Foo {}) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_bound_satisfied_val_binding() {
    // Use val binding so main returns Unit.
    let thir = check_source(
        "trait Ord {}
struct Foo {}
impl Ord for Foo {}
fn take_ord<T: Ord>(let x: T) -> T { x }
fn main() { val _x = take_ord(Foo {}) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        thir.diagnostics
    );
}

// ── Bound unsatisfied ────────────────────────────────────────────────────────

#[test]
fn test_bound_unsatisfied() {
    let thir = check_source(
        "trait Ord {}
struct Foo {}
fn take_ord<T: Ord>(let x: T) -> T { x }
fn main() -> Foo { take_ord(Foo {}) }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "unsatisfied_bound"),
        "expected unsatisfied_bound diagnostic, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_bound_unsatisfied_val_binding() {
    let thir = check_source(
        "trait Ord {}
struct Foo {}
fn take_ord<T: Ord>(let x: T) -> T { x }
fn main() { val _x = take_ord(Foo {}) }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "unsatisfied_bound"),
        "expected unsatisfied_bound diagnostic, got: {:?}",
        thir.diagnostics
    );
}

// ── Multiple bounds ──────────────────────────────────────────────────────────

#[test]
fn test_multiple_bounds_all_satisfied() {
    let thir = check_source(
        "trait Hashable {}
trait Equatable {}
struct MyStr {}
impl Hashable for MyStr {}
impl Equatable for MyStr {}
fn take_both<T: Hashable + Equatable>(let x: T) -> T { x }
fn main() -> MyStr { take_both(MyStr {}) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_multiple_bounds_one_missing() {
    let thir = check_source(
        "trait Hashable {}
trait Equatable {}
struct MyStr {}
impl Hashable for MyStr {}
fn take_both<T: Hashable + Equatable>(let x: T) -> T { x }
fn main() -> MyStr { take_both(MyStr {}) }",
    );
    let unsatisfied: Vec<_> = thir
        .diagnostics
        .iter()
        .filter(|d| d.kind() == "unsatisfied_bound")
        .collect();
    assert_eq!(
        unsatisfied.len(),
        1,
        "expected exactly 1 unsatisfied bound (Equatable), got: {:?}",
        thir.diagnostics
    );
}

// ── Two type parameters ─────────────────────────────────────────────────────

#[test]
fn test_two_type_params_both_satisfied() {
    let thir = check_source(
        "trait Ord {}
struct A {}
struct B {}
impl Ord for A {}
impl Ord for B {}
fn sort_two<X: Ord, Y: Ord>(let a: X, let b: Y) -> X { a }
fn main() -> A { sort_two(A {}, B {}) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_two_type_params_one_unsatisfied() {
    let thir = check_source(
        "trait Ord {}
struct A {}
struct B {}
impl Ord for A {}
fn sort_two<X: Ord, Y: Ord>(let a: X, let b: Y) -> X { a }
fn main() -> A { sort_two(A {}, B {}) }",
    );
    let unsatisfied: Vec<_> = thir
        .diagnostics
        .iter()
        .filter(|d| d.kind() == "unsatisfied_bound")
        .collect();
    assert_eq!(
        unsatisfied.len(),
        1,
        "expected exactly 1 unsatisfied bound (B: Ord), got: {:?}",
        thir.diagnostics
    );
}

// ── No bounds = no checking ──────────────────────────────────────────────────

#[test]
fn test_no_bounds_no_checking() {
    let thir = check_source(
        "fn identity<T>(let x: T) -> T { x }
fn main() -> Int { identity(42) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        thir.diagnostics
    );
}

// ── Non-generic call ignores bounds ──────────────────────────────────────────

#[test]
fn test_nongeneric_call_ignores_bounds() {
    let thir = check_source(
        "fn add(let a: Int, let b: Int) -> Int { a }
fn main() -> Int { add(1, 2) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        thir.diagnostics
    );
}
