//! Name resolution for the Axiom compiler: assigns a `DefId` to every name
//! reference in a lowered HIR, producing a resolved `Hir`.
//!
//! Depends on `lower` for base HIR types (`Item`, `FnDef`, `HirDiagnostic`,
//! `Def`, `DefKind`, `HirId`, etc.).

// Re-export lower crate's modules so that `crate::hir`, `crate::error`,
// `crate::lowering`, `crate::serialize` paths work in this crate's source.
pub use lower::error;
pub use lower::hir_types;
pub use lower::lowering;
pub use lower::serialize;

// Re-export all lower types at crate level so `resolver::X` works for
// everything (use as replacement for now-deleted `hir::X`).
// Re-export all of `lower` so that `resolver::TypeName` works for every
// lower type. This is a deliberate glob — consumers use `resolver::` as the
// single HIR surface. Internal code in this crate uses `crate::hir` (aliased
// from `lower::hir_types`) for module-path access.
pub use lower::*;

pub mod desugar;
pub mod intrinsic;
pub mod lang;
pub mod resolve;

pub use desugar::desugar;
pub use intrinsic::{
    collect_intrinsic_bindings, validate_intrinsic_bindings, IntrinsicBinding, HEAP_ALLOC,
    HEAP_FREE, HEAP_GET, HEAP_SET, KNOWN_INTRINSICS,
};
pub use lang::{
    collect_lang_bindings, resolve_lang_items, LangBinding, LangItems, REQUIRED_LANG_ITEMS,
};
pub use resolve::{build_global_exports, resolve_with_globals, GlobalExports};

/// Full lowering + resolution in one call. Parses source, structurally lowers,
/// resolves names. Convenience for callers that don't need multi-module
/// orchestration.
pub fn lower(
    root: &parser::ast::SourceFile,
    source: &str,
    global_exports: Option<&GlobalExports>,
) -> Hir {
    let (items, defs, diags, _) = lower::lower_structural(root, source, 0);
    let mut ctx = lower::lowering::LowerCtx::new(source);
    ctx.items = items;
    ctx.defs = defs;
    ctx.diagnostics = diags;
    resolve::resolve(&mut ctx, global_exports);
    Hir {
        items: ctx.items,
        diagnostics: ctx.diagnostics,
    }
}
