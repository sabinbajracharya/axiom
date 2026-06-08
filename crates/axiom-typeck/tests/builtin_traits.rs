//! Integration tests for the core traits: Deinit, Equatable, Hashable, Ord.
//!
//! These are ordinary library traits declared in `stdlib/core/traits.ax`, so
//! the tests compile user source on top of the embedded stdlib (the same path
//! `forge run` uses). Their primitive auto-impls cover Int/Float/Bool/String,
//! and Deinit covers all types.

fn check_source(source: &str) -> axiom_typeck::Thir {
    axiom_typeck::check_modules(&axiom_stdlib::with_main(source))
}

// ── Deinit bound ────────────────────────────────────────────────────────────

#[test]
fn test_deinit_bound_satisfied_for_int() {
    let thir = check_source(
        "fn drop_val<T: Deinit>(let x: T) { }
fn main() { drop_val(42) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for Deinit(Int), got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_deinit_bound_satisfied_for_string() {
    let thir = check_source(
        "fn drop_val<T: Deinit>(let x: T) { }
fn main() { drop_val(\"hello\") }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for Deinit(String), got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_deinit_bound_satisfied_for_struct() {
    // Deinit auto-impls for ALL types, including user-defined structs.
    let thir = check_source(
        "struct Foo { x: Int }
fn drop_val<T: Deinit>(let x: T) { }
fn main() { drop_val(Foo { x: 1 }) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for Deinit(Foo), got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_deinit_bound_satisfied_for_nested_struct() {
    // A struct whose field is another struct — both get Deinit auto-impls.
    let thir = check_source(
        "struct Inner { v: Int }
struct Outer { inner: Inner }
fn drop_val<T: Deinit>(let x: T) { }
fn main() { drop_val(Outer { inner: Inner { v: 42 } }) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for Deinit(Outer), got: {:?}",
        thir.diagnostics
    );
}

// ── Equatable bound ─────────────────────────────────────────────────────────

#[test]
fn test_equatable_bound_satisfied_for_int() {
    let thir = check_source(
        "fn eq_test<T: Equatable>(let x: T) { }
fn main() { eq_test(42) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for Equatable(Int), got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_equatable_bound_satisfied_for_bool() {
    let thir = check_source(
        "fn eq_test<T: Equatable>(let x: T) { }
fn main() { eq_test(true) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for Equatable(Bool), got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_equatable_bound_unsatisfied_for_struct() {
    let thir = check_source(
        "struct Foo { x: Int }
fn eq_test<T: Equatable>(let x: T) { }
fn main() { eq_test(Foo { x: 1 }) }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "unsatisfied_bound"),
        "expected unsatisfied_bound for Equatable(Foo), got: {:?}",
        thir.diagnostics
    );
}

// ── Hashable bound ──────────────────────────────────────────────────────────

#[test]
fn test_hashable_bound_satisfied_for_int() {
    let thir = check_source(
        "fn hash_test<T: Hashable>(let x: T) { }
fn main() { hash_test(42) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for Hashable(Int), got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_hashable_bound_satisfied_for_string() {
    let thir = check_source(
        "fn hash_test<T: Hashable>(let x: T) { }
fn main() { hash_test(\"key\") }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for Hashable(String), got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_hashable_bound_unsatisfied_for_struct() {
    let thir = check_source(
        "struct Foo { x: Int }
fn hash_test<T: Hashable>(let x: T) { }
fn main() { hash_test(Foo { x: 1 }) }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "unsatisfied_bound"),
        "expected unsatisfied_bound for Hashable(Foo), got: {:?}",
        thir.diagnostics
    );
}

// ── Ord bound ───────────────────────────────────────────────────────────────

#[test]
fn test_ord_bound_satisfied_for_float() {
    let thir = check_source(
        "fn sort<T: Ord>(let x: T) { }
fn main() { sort(1.0) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for Ord(Float), got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_ord_bound_unsatisfied_for_struct() {
    let thir = check_source(
        "struct Foo { x: Int }
fn sort<T: Ord>(let x: T) { }
fn main() { sort(Foo { x: 1 }) }",
    );
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "unsatisfied_bound"),
        "expected unsatisfied_bound for Ord(Foo), got: {:?}",
        thir.diagnostics
    );
}

// ── Supertrait propagation ──────────────────────────────────────────────────

#[test]
fn test_hashable_implies_equatable_for_int() {
    // Hashable requires Equatable. Int has both auto-impls, so this passes.
    let thir = check_source(
        "fn both<T: Hashable>(let x: T) { }
fn main() { both(42) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_hashable_implies_equatable_for_struct_without_equatable() {
    // A struct that has Hashable but not Equatable should fail the
    // supertrait check. In practice this can't happen with auto-impls
    // (they always register both), but a user could write a partial impl.
    // Since we don't support user impl overriding built-in yet, this test
    // verifies the built-in Hashable for a struct that doesn't have it.
    let thir = check_source(
        "struct Foo { x: Int }
fn both<T: Hashable>(let x: T) { }
fn main() { both(Foo { x: 1 }) }",
    );
    // Foo has neither Hashable nor Equatable — should fail.
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "unsatisfied_bound"),
        "expected unsatisfied_bound for Hashable(Foo), got: {:?}",
        thir.diagnostics
    );
}

// ── Multiple built-in bounds ────────────────────────────────────────────────

#[test]
fn test_multiple_builtin_bounds_all_satisfied() {
    let thir = check_source(
        "fn use_both<T: Equatable + Hashable>(let x: T) { }
fn main() { use_both(42) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for Equatable+Hashable(Int), got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_multiple_builtin_bounds_one_missing() {
    // Foo has no impls at all.
    let thir = check_source(
        "struct Foo { x: Int }
fn use_both<T: Equatable + Hashable>(let x: T) { }
fn main() { use_both(Foo { x: 1 }) }",
    );
    let unsatisfied: Vec<_> = thir
        .diagnostics
        .iter()
        .filter(|d| d.kind() == "unsatisfied_bound")
        .collect();
    assert!(
        unsatisfied.len() >= 2,
        "expected at least 2 unsatisfied bounds (Equatable, Hashable), got: {:?}",
        thir.diagnostics
    );
}

// ── User-defined trait + impl satisfies a bound ──────────────────────────────

#[test]
fn test_user_defined_trait_satisfies_bound() {
    // A user trait with an explicit impl satisfies a bound on that trait,
    // alongside the core traits from stdlib.
    let thir = check_source(
        "trait Drawable {}
struct Foo {}
impl Drawable for Foo {}
fn draw<T: Drawable>(let x: T) { }
fn main() { draw(Foo {}) }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for user-defined trait bound, got: {:?}",
        thir.diagnostics
    );
}

// ── Subscript declarations ───────────────────────────────────────────────────

#[test]
fn test_subscript_on_struct() {
    // A struct with a subscript definition can be indexed.
    let thir = check_source(
        "struct Box { v: Int }
impl Box {
    subscript(let self, i: Int) -> Int { yield self.v }
}
fn main() { val b = Box { v: 7 }; val x = b[0] }",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "expected no diagnostics for subscript, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_subscript_then_method_on_same_type() {
    // After subscript resolution on a type, a subsequent method call on the
    // same type must resolve correctly (verifies with_type_params restore).
    let thir = check_source(
        "fn main() {
    val xs: List<Int> = [1, 2]
    val a = xs[0]
    xs.push(3)
    val b = xs[1]
}",
    );
    assert!(
        thir.diagnostics.is_empty(),
        "subscript then method: {:?}",
        thir.diagnostics
    );
}
