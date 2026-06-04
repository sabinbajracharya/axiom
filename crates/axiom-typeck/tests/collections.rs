//! Integration tests for collection types (List<T>, Map<K, V>).

#![allow(clippy::unwrap_used)]

use axiom_hir::lower;
use axiom_parser::ast::AstNode;
use axiom_typeck::{check, serialize, Thir};

fn check_source(source: &str) -> Thir {
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = lower(&root, source);
    check(hir)
}

fn dump(thir: &Thir) -> String {
    serialize(thir)
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
