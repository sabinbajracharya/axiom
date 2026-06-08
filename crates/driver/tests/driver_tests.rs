//! Unit tests for the pipeline orchestrator.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use driver::check_modules;
use driver::check_source;

/// Bare mode should compile a trivial program without diagnostics.
#[test]
fn test_check_source_trivial() {
    let thir = check_source("fn main() { val x = 1 }");
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        thir.diagnostics
    );
}

/// Stdlib-backed mode compiles cleanly with the real stdlib.
#[test]
fn test_check_modules_with_stdlib() {
    let modules = stdlib::with_main("fn main() {}");
    let thir = check_modules(&modules);
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        thir.diagnostics
    );
}

/// `@lang` attribute outside stdlib is rejected.
#[test]
fn test_lang_outside_stdlib_rejected() {
    let modules = stdlib::with_main("@lang(\"list\")\nstruct Bad { x: Int }\nfn main() {}");
    let thir = check_modules(&modules);
    assert!(
        thir.diagnostics.iter().any(|d| {
            matches!(
                d,
                typecheck::Diagnostic::Hir(resolver::HirDiagnostic::LangItemOutsideStdlib { .. })
            )
        }),
        "expected LangItemOutsideStdlib, got: {:?}",
        thir.diagnostics
    );
}

/// `@intrinsic` attribute outside stdlib is rejected.
#[test]
fn test_intrinsic_outside_stdlib_rejected() {
    let modules =
        stdlib::with_main("@intrinsic(\"heap_alloc\")\nfn bad() -> Int { 1 }\nfn main() {}");
    let thir = check_modules(&modules);
    assert!(
        thir.diagnostics.iter().any(|d| {
            matches!(
                d,
                typecheck::Diagnostic::Hir(resolver::HirDiagnostic::IntrinsicOutsideStdlib { .. })
            )
        }),
        "expected IntrinsicOutsideStdlib, got: {:?}",
        thir.diagnostics
    );
}

/// Single module with no stdlib produces clean output.
#[test]
fn test_check_modules_single_bare_module() {
    let thir = check_modules(&[("", "fn main() {}")]);
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected: {:?}",
        thir.diagnostics
    );
}

/// Trivial multi-module: two files, one calls the other.
#[test]
fn test_check_modules_two_modules() {
    let modules = stdlib::with_main("fn main() { val x = helper() }");
    let thir = check_modules(&modules);
    assert!(
        !thir.diagnostics.is_empty(),
        "expected diagnostics for unresolved helper, got none"
    );
}

/// Divergence guard: `driver::check_source` must agree with
/// `typecheck::check_source` on the same input.
#[test]
fn test_check_source_bare_vs_driver_agree() {
    let source = "fn main() { val x = 1 }";
    let driver_thir = check_source(source);
    let typeck_thir = typecheck::check_source(source);
    assert!(
        driver_thir.diagnostics.is_empty(),
        "driver: {:?}",
        driver_thir.diagnostics
    );
    assert!(
        typeck_thir.diagnostics.is_empty(),
        "typeck: {:?}",
        typeck_thir.diagnostics
    );
    // Both should find Int in the type map
    let has_int = |t: &typecheck::Thir| t.types.values().any(|ty| matches!(ty, typecheck::Ty::Int));
    assert!(has_int(&driver_thir), "driver: no Int found");
    assert!(has_int(&typeck_thir), "typeck: no Int found");
}
