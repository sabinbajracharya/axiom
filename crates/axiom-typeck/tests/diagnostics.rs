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

// ── Snapshot-based diagnostic tests ────────────────────────────────────────

#[test]
fn test_diag_snapshot_type_mismatch() {
    check_diagnostics_snapshot("type_mismatch", "fn main() { val x: Int = 3.14 }");
}

#[test]
fn test_diag_snapshot_undefined_type() {
    check_diagnostics_snapshot("undefined_type", "fn main() { val x: Foo = 1 }");
}

#[test]
fn test_diag_snapshot_unknown_field() {
    check_diagnostics_snapshot(
        "unknown_field",
        "struct Point { x: Float, y: Float }
fn main() { val p = Point { x: 1.0, y: 2.0 } val z = p.z }",
    );
}

#[test]
fn test_diag_snapshot_unknown_variant() {
    check_diagnostics_snapshot(
        "unknown_variant",
        "enum Shape { Circle(Float), Rect(Float, Float) }
fn main() {
    val s = Rect(1.0, 2.0)
    match s {
        Circle(r) => r
        Square(w) => w
    }
}",
    );
}

#[test]
fn test_diag_snapshot_call_arity_mismatch() {
    check_diagnostics_snapshot(
        "call_arity_mismatch",
        "fn add(a: Int, b: Int) -> Int { a + b }
fn main() { add(1) }",
    );
}

#[test]
fn test_diag_snapshot_struct_field_mismatch() {
    check_diagnostics_snapshot(
        "struct_field_mismatch",
        "struct Point { x: Float, y: Float }
fn main() { val p = Point { x: 1.0 } }",
    );
}

#[test]
fn test_diag_snapshot_non_exhaustive_match() {
    check_diagnostics_snapshot(
        "non_exhaustive_match",
        "enum Shape { Circle(Float), Rect(Float, Float) }
fn area(s: Shape) -> Float { match s { Circle(r) => r } }",
    );
}

#[test]
fn test_diag_snapshot_guarded_non_exhaustive() {
    check_diagnostics_snapshot(
        "guarded_non_exhaustive",
        "enum Shape { Circle(Float), Rect(Float, Float) }
fn area(s: Shape) -> Float {
    match s {
        Circle(r) if r > 0.0 => r
    }
}",
    );
}

#[test]
fn test_diag_snapshot_match_arm_type_mismatch() {
    check_diagnostics_snapshot(
        "match_arm_type_mismatch",
        "enum Shape { Circle(Float), Rect(Float, Float) }
fn area(s: Shape) -> Float {
    match s {
        Circle(r) => r
        Rect(w, h) => w
    }
}",
    );
}

#[test]
fn test_diag_snapshot_if_branch_mismatch() {
    check_diagnostics_snapshot(
        "if_branch_mismatch",
        "fn main() { val x = if true { 1 } else { 2.0 } }",
    );
}

#[test]
fn test_diag_snapshot_not_callable() {
    check_diagnostics_snapshot("not_callable", "fn main() { val x = 1 x() }");
}

#[test]
fn test_diag_snapshot_assign_to_immutable() {
    check_diagnostics_snapshot("assign_to_immutable", "fn main() { val x = 1 x = 2 }");
}

#[test]
fn test_diag_snapshot_return_type_mismatch() {
    check_diagnostics_snapshot("return_type_mismatch", "fn main() -> Int { 3.14 }");
}
