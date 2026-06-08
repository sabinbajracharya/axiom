//! Name resolution for the Axiom compiler: assigns a `DefId` to every name
//! reference in a lowered HIR, producing a resolved `Hir`.
//!
//! Depends on `lower` for base HIR types (`Item`, `FnDef`, `HirDiagnostic`,
//! `Def`, `DefKind`, `HirId`, etc.).

// Re-export lower crate's modules so that `crate::hir`, `crate::error`,
// `crate::lowering`, `crate::serialize` paths work in this crate's source.
pub use lower::error;
pub use lower::hir_types as hir;
pub use lower::lowering;
pub use lower::serialize;

// Re-export commonly-used types at crate level.
pub use lower::{
    Block, CallingConvention, Def, DefKind, Expr, FnDef, Hir, HirDiagnostic, HirId, Item, NameRef,
    Stmt, lower_structural,
};

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
