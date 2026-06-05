//! Golden snapshot tests for the IR layer.
//!
//! Each test lowers a source program through the full pipeline
//! (parse → resolve → typeck → IR lower → serialize) and compares
//! the output to a checked-in `.ir` golden file.
//!
//! Set `UPDATE_SNAPSHOTS=1` to regenerate goldens.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_parser::ast::AstNode;
use std::fs;
use std::path::Path;

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn ir_source(source: &str) -> String {
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = axiom_hir::lower(&root, source);
    let thir = axiom_typeck::check(hir);
    let mono = axiom_typeck::monomorphize(&thir);
    let ir = axiom_ir::lower(&thir, &mono);
    axiom_ir::serialize(&ir)
}

fn check_golden(name: &str, source: &str) {
    let actual = ir_source(source);
    let golden_path = format!("tests/fixtures/{}.ir", name);
    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        fs::write(&golden_path, &actual).expect("write golden");
    } else {
        let expected = fs::read_to_string(&golden_path)
            .unwrap_or_else(|_| panic!("golden file missing: {golden_path}"));
        assert_eq!(
            normalize(&actual),
            normalize(&expected),
            "golden mismatch for {name}"
        );
    }
}

fn check_golden_glob() {
    let fixtures_dir = Path::new("tests/fixtures");
    let entries: Vec<_> = fs::read_dir(fixtures_dir)
        .expect("read fixtures dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "ax"))
        .collect();

    for entry in entries {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_str().unwrap();
        let source = fs::read_to_string(&path).expect("read fixture");
        check_golden(name, &source);
    }
}

#[test]
fn test_golden_glob() {
    check_golden_glob();
}

// ── Individual golden tests (inline sources) ─────────────────────────────────

#[test]
fn test_golden_hello() {
    check_golden(
        "hello",
        r#"
fn main() {
    print("Hello, Axiom!")
}
"#,
    );
}

#[test]
fn test_golden_arithmetic() {
    check_golden(
        "arithmetic",
        r#"
fn main() {
    val a = 1 + 2
    val b = a * 3
    val c = -b
}
"#,
    );
}

#[test]
fn test_golden_functions() {
    check_golden(
        "functions",
        r#"
fn add(let a: Int, let b: Int) -> Int {
    a + b
}

fn main() {
    val result = add(1, 2)
}
"#,
    );
}

#[test]
fn test_golden_control_flow() {
    check_golden(
        "control_flow",
        r#"
fn main() {
    val x = 1
    if x > 0 {
        print("positive")
    } else {
        print("non-positive")
    }
}
"#,
    );
}

#[test]
fn test_golden_loops() {
    check_golden(
        "loops",
        r#"
fn main() {
    var i = 0
    loop if i < 10 {
        i = i + 1
    }
    val x = loop {
        break 42
    }
}
"#,
    );
}

#[test]
fn test_golden_match() {
    check_golden(
        "match_expr",
        r#"
fn pick(val x: Int) -> Int {
    match x {
        0 => 1
        1 => 2
        _ => 3
    }
}

fn main() {
    val result = pick(0)
}
"#,
    );
}

#[test]
fn test_golden_multi_fn() {
    check_golden(
        "multi_fn",
        r#"
fn square(x: Int) -> Int { x * x }
fn sum_of_squares(a: Int, b: Int) -> Int { square(a) + square(b) }

fn main() {
    val result = sum_of_squares(3, 4)
}
"#,
    );
}

#[test]
fn test_golden_generics() {
    check_golden(
        "generics",
        r#"
fn id<T>(x: T) -> T {
    x
}

fn main() {
    val a = id(42)
    val b = id(true)
}
"#,
    );
}
