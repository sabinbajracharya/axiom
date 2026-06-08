//! Coverage invariant tests for the IR layer.
//!
//! Verifies structural correctness of the IR for every fixture.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use parser::ast::AstNode;
use std::fs;
use std::path::Path;

fn lower_fixture(source: &str) -> ir::Ir {
    let result = parser::parse(source);
    let root = parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = hir::lower(&root, source, None);
    let thir = typecheck::check(hir);
    let mono = typecheck::monomorphize(&thir);
    ir::lower(&thir, &mono)
}

#[test]
fn test_invariants_all_fixtures() {
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
        let ir = lower_fixture(&source);
        let errors = ir::check_invariants(&ir);
        assert!(
            errors.is_empty(),
            "invariant violations in {}:\n{}",
            name,
            errors.join("\n")
        );
    }
}

#[test]
fn test_invariants_hello() {
    let source = r#"
fn main() {
    print("Hello")
}
"#;
    let ir = lower_fixture(source);
    let errors = ir::check_invariants(&ir);
    assert!(
        errors.is_empty(),
        "invariant violations:\n{}",
        errors.join("\n")
    );
}

#[test]
fn test_invariants_empty_program() {
    let source = "";
    let ir = lower_fixture(source);
    // Empty program has no functions, so no violations.
    let errors = ir::check_invariants(&ir);
    assert!(errors.is_empty());
}
