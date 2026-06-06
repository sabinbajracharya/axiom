//! Integration tests for traits phase 2: impl checking and method dispatch.

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

// ── Impl completeness ────────────────────────────────────────────────────────

#[test]
fn test_impl_completeness_ok() {
    let thir = check_source(
        "trait Shape { fn area(self) -> Float }
struct Circle { r: Float }
impl Shape for Circle { fn area(self) -> Float { 3.14 } }
fn main() -> Float { val c = Circle { r: 1.0 } c.area() }",
    );
    let missing: Vec<_> = thir
        .diagnostics
        .iter()
        .filter(|d| d.kind() == "missing_trait_method")
        .collect();
    assert!(
        missing.is_empty(),
        "expected no missing methods, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_impl_missing_required_method() {
    let thir = check_source(
        "trait Shape { fn area(self) -> Float fn perimeter(self) -> Float }
struct Circle { r: Float }
impl Shape for Circle { fn area(self) -> Float { 3.14 } }
fn main() { }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "missing_trait_method"),
        "expected missing_trait_method diagnostic, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_impl_default_method_inherited() {
    let thir = check_source(
        "trait Greet { fn greet(self) -> String { \"hello\" } }
struct Person { name: String }
impl Greet for Person { }
fn main() -> String { val p = Person { name: \"A\" } p.greet() }",
    );
    // Default methods are not required — the impl can be empty — and calling the
    // inherited default (`p.greet()`) dispatches to the trait body.
    assert!(
        thir.diagnostics.is_empty(),
        "expected default method to be inherited and dispatched, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_default_method_dispatch_through_bound() {
    // A default method called through a generic bound (`T: Greet`) resolves via
    // the trait's default body — the concrete impl need not override it.
    let thir = check_source(
        "trait Greet { fn greet(self) -> String { \"hi\" } }
struct Person { name: String }
impl Greet for Person { }
fn run<T: Greet>(x: T) -> String { x.greet() }
fn main() -> String { val p = Person { name: \"A\" } run(p) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected default dispatch through bound, got: {:?}",
        thir.diagnostics
    );
}

// ── Method dispatch ──────────────────────────────────────────────────────────

#[test]
fn test_method_dispatch_on_struct() {
    let thir = check_source(
        "struct Circle { r: Float }
impl Circle { fn area(self) -> Float { 3.14 } }
fn main() -> Float { val c = Circle { r: 1.0 } c.area() }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .all(|d| d.kind() != "unknown_method"),
        "expected method dispatch to work, got: {:?}",
        thir.diagnostics
    );
    // The call expression should have type Float.
    let has_float = thir.types.values().any(|t| *t == axiom_typeck::Ty::Float);
    assert!(has_float, "expected Float type from method call");
}

#[test]
fn test_method_dispatch_wrong_arg_type() {
    let thir = check_source(
        "struct Circle { r: Float }
impl Circle { fn grow(self, amount: Float) -> Circle { self } }
fn main() -> Circle { val c = Circle { r: 1.0 } c.grow(true) }",
    );
    assert!(
        thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "expected type_mismatch for wrong arg type, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_method_dispatch_arity_mismatch() {
    let thir = check_source(
        "struct Circle { r: Float }
impl Circle { fn grow(self, amount: Float) -> Circle { self } }
fn main() -> Circle { val c = Circle { r: 1.0 } c.grow() }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "call_arity_mismatch"),
        "expected call_arity_mismatch, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_method_dispatch_unknown_method() {
    let thir = check_source(
        "struct Circle { r: Float }
fn main() -> Float { val c = Circle { r: 1.0 } c.nonexistent() }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "unknown_method"),
        "expected unknown_method diagnostic, got: {:?}",
        thir.diagnostics
    );
}

// ── Trait impl method dispatch ───────────────────────────────────────────────

#[test]
fn test_trait_impl_method_dispatch() {
    let thir = check_source(
        "trait Shape { fn area(self) -> Float }
struct Circle { r: Float }
impl Shape for Circle { fn area(self) -> Float { 3.14 } }
fn main() -> Float { val c = Circle { r: 1.0 } c.area() }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        thir.diagnostics
    );
}

// ── Self type ────────────────────────────────────────────────────────────────

#[test]
fn test_self_type_in_impl_body() {
    let thir = check_source(
        "struct Circle { r: Float }
impl Circle { fn new(r: Float) -> Self { Circle { r: r } } }
fn main() -> Circle { Circle::new(5.0) }",
    );
    // Self in return type resolves to Circle. The struct literal uses Circle directly
    // (Self as a struct literal expression is a future enhancement).
    let has_struct = thir
        .types
        .values()
        .any(|t| matches!(t, axiom_typeck::Ty::Struct(_)));
    assert!(has_struct, "expected Struct type from Self resolution");
}

// ── Inherent impl ────────────────────────────────────────────────────────────

#[test]
fn test_inherent_impl_method() {
    let thir = check_source(
        "struct Counter { n: Int }
impl Counter { fn increment(self) -> Int { self.n + 1 } }
fn main() -> Int { val c = Counter { n: 0 } c.increment() }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected inherent impl method to work, got: {:?}",
        thir.diagnostics
    );
}

// ── Trait not found ──────────────────────────────────────────────────────────

#[test]
fn test_trait_not_found() {
    let thir = check_source(
        "struct Circle { r: Float }
impl UnknownTrait for Circle { }
fn main() { }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "trait_not_found"),
        "expected trait_not_found diagnostic, got: {:?}",
        thir.diagnostics
    );
}

// ── Type not found for impl ──────────────────────────────────────────────────

#[test]
fn test_type_not_found_for_impl() {
    let thir = check_source(
        "impl UnknownType { fn foo(self) { } }
fn main() { }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "type_not_found_for_impl"),
        "expected type_not_found_for_impl diagnostic, got: {:?}",
        thir.diagnostics
    );
}
