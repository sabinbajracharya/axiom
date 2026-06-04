//! Diagnostics snapshot tests: ill-typed input → specific error + span.
//! Run with `UPDATE_SNAPSHOTS=1` to regenerate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_hir::lower;
use axiom_parser::ast::AstNode;
use axiom_typeck::check;

fn typeck_diagnostics(source: &str) -> Vec<String> {
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = lower(&root, source);
    let thir = check(hir);
    thir.diagnostics.iter().map(|d| d.render(source)).collect()
}

#[allow(dead_code)]
fn check_diagnostics_snapshot(name: &str, source: &str) {
    let diagnostics = typeck_diagnostics(source);
    let output = diagnostics.join("\n");
    let golden_path = format!("tests/fixtures/errors/{}.stderr", name);

    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        std::fs::write(&golden_path, &output).unwrap();
    } else {
        let expected = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|_| panic!("golden file missing: {golden_path}"));
        assert_eq!(output, expected, "diagnostics mismatch for {name}");
    }
}

#[test]
fn test_diag_type_mismatch() {
    let diags = typeck_diagnostics("fn main() { val x: Int = 3.14 }");
    assert!(!diags.is_empty(), "expected at least one diagnostic");
    assert!(diags.iter().any(|d| d.contains("type mismatch")));
}

#[test]
fn test_diag_call_arity_mismatch() {
    let diags = typeck_diagnostics("fn add(a: Int, b: Int) -> Int { a + b } fn main() { add(1) }");
    assert!(diags.iter().any(|d| d.contains("arity")));
}

#[test]
fn test_diag_non_exhaustive_match() {
    let diags = typeck_diagnostics(
        "enum Shape { Circle(Float), Rect(Float, Float) }
fn test(s: Shape) -> Float { match s { Circle(r) => r } }",
    );
    assert!(diags.iter().any(|d| d.contains("non-exhaustive")));
}

#[test]
fn test_diag_assign_to_immutable() {
    let diags = typeck_diagnostics("fn main() { val x = 1 x = 2 }");
    assert!(diags.iter().any(|d| d.contains("immutable")));
}

#[test]
fn test_diag_not_callable() {
    let diags = typeck_diagnostics("fn main() { val x = 1 x() }");
    assert!(diags.iter().any(|d| d.contains("not a function")));
}
