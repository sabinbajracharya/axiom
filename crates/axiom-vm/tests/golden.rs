//! Golden trace tests for the VM.
//!
//! Each test runs a source program through the full pipeline
//! (parse → resolve → typeck → IR lower → VM with tracing) and compares
//! the execution trace to a checked-in `.trace` golden file.
//!
//! Set `UPDATE_SNAPSHOTS=1` to regenerate goldens.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn run_with_trace(source: &str) -> String {
    // Compile the user source on top of the embedded stdlib through the one
    // unified pipeline (the same path single-file `forge run` uses). `print`/
    // `println` resolve to the real `stdlib/std/io.ax` functions — there are no
    // print/println VM builtins. See `docs/stdlib-loading-unification.md`.
    let thir = axiom_typeck::check_modules(&axiom_stdlib::with_main(source));
    // Execution fixtures must type-check cleanly. This guard is what surfaces
    // bugs like passing a non-`String` to the `String`-only `print` — previously
    // such diagnostics were silently ignored here. See
    // `docs/string-format-and-print-retire.md`.
    assert!(
        thir.diagnostics.is_empty(),
        "fixture has type diagnostics: {:?}",
        thir.diagnostics
    );
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
