//! Golden snapshot tests: `.ax` fixtures → `.thir` goldens.
//! Run with `UPDATE_SNAPSHOTS=1` to regenerate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_hir::lower;
use axiom_parser::ast::AstNode;
use axiom_typeck::{check, serialize};

fn typeck_source(source: &str) -> String {
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = lower(&root, source);
    let thir = check(hir);
    serialize(&thir)
}

fn read_golden(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn write_golden(path: &str, content: &str) {
    std::fs::write(path, content).unwrap();
}

fn check_golden(name: &str, source: &str) {
    let actual = typeck_source(source);
    let golden_path = format!("tests/fixtures/{}.thir", name);

    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        write_golden(&golden_path, &actual);
    } else {
        let expected = read_golden(&golden_path)
            .unwrap_or_else(|| panic!("golden file missing: {golden_path}"));
        assert_eq!(actual, expected, "golden mismatch for {name}");
    }
}

#[test]
fn test_golden_hello() {
    check_golden("hello", "fn main() { print(\"Hello, Axiom!\") }");
}

#[test]
fn test_golden_arithmetic() {
    check_golden("arithmetic", "fn add(a: Int, b: Int) -> Int { a + b }");
}

#[test]
fn test_golden_simple_struct() {
    check_golden(
        "simple_struct",
        "struct Point { x: Float, y: Float }
fn main() { val p = Point { x: 1.0, y: 2.0 } }",
    );
}

#[test]
fn test_golden_simple_enum() {
    check_golden(
        "simple_enum",
        "enum Shape { Circle(Float), Rect(Float, Float), Empty }",
    );
}

#[test]
fn test_golden_type_mismatch() {
    check_golden("type_mismatch", "fn main() { val x: Int = 3.14 }");
}
