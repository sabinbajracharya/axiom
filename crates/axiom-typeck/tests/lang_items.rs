//! Lang-item registry integration tests
//! (`docs/lang-items-and-desugaring-design.md` §6.2, §6.5).
//!
//! These exercise the *real* embedded stdlib through `check_modules`, so they
//! prove the compiler→stdlib binding holds end to end: the required lang items
//! resolve to exactly one stdlib def each, and `@lang` tags outside the stdlib
//! are rejected.

#![allow(clippy::unwrap_used)]

use axiom_hir::HirDiagnostic;
use axiom_typeck::Thir;

fn compile(source: &str) -> Thir {
    axiom_driver::check_modules(&axiom_stdlib::with_main(source))
}

fn lang_diagnostics(thir: &Thir) -> Vec<&axiom_hir::HirDiagnostic> {
    thir.diagnostics
        .iter()
        .filter_map(|d| {
            if let axiom_typeck::Diagnostic::Hir(hir_diag) = d {
                Some(hir_diag)
            } else {
                None
            }
        })
        .collect()
}

/// §6.2 — registry completeness on the real stdlib: every required lang item
/// resolves to exactly one def. A missing/duplicate/orphan binding would surface
/// here as a diagnostic, so a clean compile *is* the consistency guarantee.
#[test]
fn test_stdlib_binds_every_required_lang_item_exactly_once() {
    let thir = compile("fn main() {}");
    let diags = lang_diagnostics(&thir);
    assert!(
        diags.is_empty(),
        "stdlib lang-item binding is inconsistent: {diags:?}"
    );
}

/// A list literal compiles cleanly against the real stdlib — the registry
/// resolved `list` to its real def, so `infer_list_lit` no longer fabricates an
/// `HirId(0)` (§3.2).
#[test]
fn test_list_literal_compiles_clean_with_real_lang_items() {
    let thir = compile("fn main() { val xs = [1, 2, 3] }");
    assert!(
        lang_diagnostics(&thir).is_empty(),
        "unexpected lang diagnostics: {:?}",
        lang_diagnostics(&thir)
    );
}

/// §6.5 — a `@lang` tag in user code (not the stdlib) is rejected, so user code
/// cannot hijack a compiler lang item.
#[test]
fn test_lang_attribute_outside_stdlib_is_rejected() {
    let thir = compile("@lang(\"list\")\nstruct Sneaky { x: Int }\nfn main() {}");
    let diags = lang_diagnostics(&thir);
    assert!(
        diags.iter().any(
            |d| matches!(d, HirDiagnostic::LangItemOutsideStdlib { key, .. } if key == "list")
        ),
        "expected LangItemOutsideStdlib, got {diags:?}"
    );
}
