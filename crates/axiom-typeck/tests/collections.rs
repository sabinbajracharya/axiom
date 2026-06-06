//! Integration tests for collection types (List<T>, Map<K, V>).
//!
//! Compiles each source on the embedded stdlib via
//! `check_modules(axiom_stdlib::with_main(..))`, so List methods (count,
//! is_empty, etc.) resolve from the library definition (loaded with bodies)
//! rather than relying solely on compiler built-ins.

#![allow(clippy::unwrap_used)]

use axiom_typeck::{serialize, Thir};

fn check_source(source: &str) -> Thir {
    axiom_typeck::check_modules(&axiom_stdlib::with_main(source))
}

fn dump(thir: &Thir) -> String {
    serialize(thir, None)
}

#[test]
fn test_list_literal_infers_list_int() {
    let thir = check_source("fn main() { val xs = [1, 2, 3] }");
    assert!(
        !thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "unexpected errors: {:?}",
        thir.diagnostics
    );
    let d = dump(&thir);
    assert!(d.contains("List"), "expected List type in dump:\n{d}");
}

#[test]
fn test_list_literal_infers_list_string() {
    let thir = check_source(r#"fn main() { val xs = ["a", "b"] }"#);
    assert!(
        !thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "unexpected errors: {:?}",
        thir.diagnostics
    );
    let d = dump(&thir);
    assert!(d.contains("List"), "expected List type in dump:\n{d}");
}

#[test]
fn test_list_literal_type_mismatch() {
    let thir = check_source(r#"fn main() { val xs = [1, "hello"] }"#);
    assert!(
        thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "expected type_mismatch for mixed list, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_list_literal_empty_requires_annotation() {
    let thir = check_source("fn main() { val xs = [] }");
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "not_yet_supported"),
        "expected not_yet_supported for empty list, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_list_push_method() {
    let thir = check_source("fn main() { var xs = [1, 2, 3] xs.push(4) }");
    assert!(
        !thir
            .diagnostics
            .iter()
            .any(|d| d.kind() == "unknown_method"),
        "expected push to resolve, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_list_count_method() {
    let thir = check_source("fn main() { val xs = [1, 2, 3] val n = xs.count() }");
    assert!(
        !thir
            .diagnostics
            .iter()
            .any(|d| d.kind() == "unknown_method"),
        "expected count to resolve, got: {:?}",
        thir.diagnostics
    );
    assert!(
        thir.types.values().any(|t| *t == axiom_typeck::Ty::Int),
        "expected count() to return Int"
    );
}

#[test]
fn test_list_is_empty_method() {
    let thir = check_source("fn main() { val xs = [1, 2] val e = xs.is_empty() }");
    assert!(
        !thir
            .diagnostics
            .iter()
            .any(|d| d.kind() == "unknown_method"),
        "expected is_empty to resolve, got: {:?}",
        thir.diagnostics
    );
    assert!(
        thir.types.values().any(|t| *t == axiom_typeck::Ty::Bool),
        "expected is_empty() to return Bool"
    );
}

#[test]
fn test_list_index_expression() {
    let thir = check_source("fn main() { val xs = [1, 2, 3] val x = xs[0] }");
    assert!(
        !thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "unexpected errors: {:?}",
        thir.diagnostics
    );
    assert!(
        thir.types.values().any(|t| *t == axiom_typeck::Ty::Int),
        "expected xs[0] to return Int"
    );
}

#[test]
fn test_list_capacity_method() {
    let thir = check_source("fn main() { val xs = [1, 2] val c = xs.capacity() }");
    assert!(
        !thir
            .diagnostics
            .iter()
            .any(|d| d.kind() == "unknown_method"),
        "expected capacity to resolve, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_unknown_method_on_list() {
    let thir = check_source("fn main() { val xs = [1, 2, 3] xs.foo() }");
    assert!(
        thir.diagnostics
            .iter()
            .any(|d| d.kind() == "unknown_method"),
        "expected unknown_method for foo(), got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_empty_list_literal_with_annotation_is_ok() {
    let thir = check_source("fn main() { val xs: List<Int> = [] }");
    assert!(
        thir.hir.diagnostics.is_empty() && thir.diagnostics.is_empty(),
        "unexpected diagnostics: hir={:?} typeck={:?}",
        thir.hir.diagnostics,
        thir.diagnostics
    );
    let d = dump(&thir);
    assert!(d.contains("List"), "expected List type in dump:\n{d}");
}

#[test]
fn test_empty_list_literal_without_annotation_is_rejected() {
    let thir = check_source("fn main() { val xs = [] }");
    assert!(
        !thir.hir.diagnostics.is_empty() || !thir.diagnostics.is_empty(),
        "expected an ambiguity diagnostic for unannotated empty list"
    );
}

#[test]
fn test_empty_list_literal_with_non_list_annotation_is_type_error() {
    let thir = check_source("fn main() { val xs: Int = [] }");
    assert!(
        thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "expected type_mismatch, got: {:?}",
        thir.diagnostics
    );
}
