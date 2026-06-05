//! Golden trace tests for the VM.
//!
//! Each test runs a source program through the full pipeline
//! (parse → resolve → typeck → IR lower → VM with tracing) and compares
//! the execution trace to a checked-in `.trace` golden file.
//!
//! Set `UPDATE_SNAPSHOTS=1` to regenerate goldens.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_parser::ast::AstNode;
use std::fs;
use std::path::Path;

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn run_with_trace(source: &str) -> String {
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = axiom_hir::lower(&root, source, None);

    let thir = axiom_typeck::check(hir);
    let mono = axiom_typeck::monomorphize(&thir);
    let ir = axiom_ir::lower(&thir, &mono);

    let mut vm = axiom_vm::Vm::new(ir);
    vm.set_tracing(true);
    let _ = vm.run();
    vm.take_trace().map(|t| t.format()).unwrap_or_default()
}

fn check_golden(name: &str, source: &str) {
    let actual = run_with_trace(source);
    let golden_path = format!("tests/fixtures/{}.trace", name);
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
