//! The Axiom HIR — re-export façade that combines `axiom-lower` (structural
//! lowering + base HIR types) and `axiom-resolver` (name resolution + lang/
//! intrinsic items + desugar) into one public API.
//!
//! All existing `axiom_hir::*` import paths remain valid.

// Re-export the lower crate (base types, HirDiagnostic, check_all, etc.)
pub use axiom_lower::*;

// Re-export resolver sub-modules explicitly so `axiom_hir::intrinsic`,
// `axiom_hir::lang`, etc. paths still work.
pub use axiom_resolver::desugar;
pub use axiom_resolver::intrinsic;
pub use axiom_resolver::lang;
pub use axiom_resolver::resolve;

// Re-export commonly-used resolver types and constants at the top level.
pub use axiom_resolver::intrinsic::{
    collect_intrinsic_bindings, validate_intrinsic_bindings, IntrinsicBinding, HEAP_ALLOC,
    HEAP_FREE, HEAP_GET, HEAP_SET, KNOWN_INTRINSICS,
};
pub use axiom_resolver::lang::{
    collect_lang_bindings, resolve_lang_items, LangBinding, LangItems, REQUIRED_LANG_ITEMS,
};
pub use axiom_resolver::resolve::{build_global_exports, resolve_with_globals, GlobalExports};

/// Full lowering + resolution in one call. Parses source, structurally lowers,
/// resolves names.
pub fn lower(
    root: &axiom_parser::ast::SourceFile,
    source: &str,
    global_exports: Option<&GlobalExports>,
) -> axiom_lower::Hir {
    let (items, defs, diags, _) = axiom_lower::lower_structural(root, source, 0);
    let mut ctx = axiom_lower::lower::LowerCtx::new(source);
    ctx.items = items;
    ctx.defs = defs;
    ctx.diagnostics = diags;
    axiom_resolver::resolve::resolve(&mut ctx, global_exports);
    axiom_lower::Hir {
        items: ctx.items,
        diagnostics: ctx.diagnostics,
    }
}
