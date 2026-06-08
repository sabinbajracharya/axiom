//! The Axiom HIR — re-export façade combining `lower` (structural lowering +
//! base types) and `resolver` (name resolution + lang/intrinsic + desugar).
//!
//! All existing `hir::*` import paths remain valid. The `lower` crate provides
//! all base types (HirId, Item, FnDef, etc.) and the `resolver` crate provides
//! name resolution, lang items, intrinsics, and desugar.

// ── Lower crate: full re-export (all base HIR types) ───────────────────
// Use `lower::*` to bring everything in, then override with explicit items
// to resolve `hir` module vs other name collisions.
pub use lower::*;

// ── Resolver: override modules (these take priority over lower's glob) ──
pub use resolver::desugar;
pub use resolver::intrinsic;
pub use resolver::lang;
pub use resolver::resolve;

// ── Resolver: top-level types ───────────────────────────────────────────
pub use resolver::intrinsic::{
    collect_intrinsic_bindings, validate_intrinsic_bindings, IntrinsicBinding, HEAP_ALLOC,
    HEAP_FREE, HEAP_GET, HEAP_SET, KNOWN_INTRINSICS,
};
pub use resolver::lang::{
    collect_lang_bindings, resolve_lang_items, LangBinding, LangItems, REQUIRED_LANG_ITEMS,
};
pub use resolver::resolve::{build_global_exports, resolve_with_globals, GlobalExports};

/// Full lowering + resolution in one call.
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
    resolver::resolve::resolve(&mut ctx, global_exports);
    Hir {
        items: ctx.items,
        diagnostics: ctx.diagnostics,
    }
}
