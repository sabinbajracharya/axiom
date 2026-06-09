//! Orphan rule integration tests (§3.5).
//!
//! Exercises `driver::check_modules` with the real embedded stdlib so the
//! `def_origins` map is populated and the orphan check bites.

#![allow(clippy::unwrap_used)]

use typecheck::Thir;

fn compile(source: &str) -> Thir {
    driver::check_modules(&stdlib::with_main(source))
}

/// User code can implement a user-defined trait for a user-defined type.
#[test]
fn test_orphan_allowed_user_trait_for_stdlib_type() {
    let thir = compile(
        "trait MyTrait { fn foo(self) -> Int; }
struct MyStruct { x: Int }
impl MyTrait for MyStruct { fn foo(self) -> Int { self.x } }
fn main() { }",
    );
    let orphan: Vec<_> = thir
        .diagnostics
        .iter()
        .filter(|d| d.kind() == "orphan_impl")
        .collect();
    assert!(
        orphan.is_empty(),
        "expected no orphan diagnostic for user trait on user type, got: {orphan:?}"
    );
}

/// User code CANNOT implement a stdlib trait for a stdlib type (owns neither).
#[test]
fn test_orphan_denied_stdlib_trait_for_stdlib_type() {
    let thir = compile(
        "impl Equatable for Int { fn eq(let self, other: Int) -> Bool { true } }
fn main() { }",
    );
    let has_orphan = thir.diagnostics.iter().any(|d| d.kind() == "orphan_impl");
    assert!(
        has_orphan,
        "expected orphan_impl diagnostic for stdlib trait on stdlib type, got: {:?}",
        thir.diagnostics
    );
}

/// User code can implement a stdlib trait for a user-defined type (owns the type).
#[test]
fn test_orphan_allowed_stdlib_trait_for_user_type() {
    let thir = compile(
        "struct MyStruct { x: String }
impl Deinit for MyStruct { }
fn main() { }",
    );
    let orphan: Vec<_> = thir
        .diagnostics
        .iter()
        .filter(|d| d.kind() == "orphan_impl")
        .collect();
    assert!(
        orphan.is_empty(),
        "expected no orphan diagnostic for stdlib trait on user type, got: {orphan:?}"
    );
}

/// Inherent impls are never subject to the orphan rule.
#[test]
fn test_orphan_inherent_impl_never_checked() {
    let thir = compile(
        "struct MyStruct { x: Int }
impl MyStruct { fn get(self) -> Int { self.x } }
fn main() { }",
    );
    let orphan: Vec<_> = thir
        .diagnostics
        .iter()
        .filter(|d| d.kind() == "orphan_impl")
        .collect();
    assert!(
        orphan.is_empty(),
        "expected no orphan diagnostic for inherent impl, got: {orphan:?}"
    );
}
