//! Name resolution for the Axiom compiler: assigns a `DefId` to every name
//! reference in a lowered HIR, producing a resolved `Hir`.
//!
//! Depends on `axiom-lower` for base HIR types (`Item`, `FnDef`, `HirDiagnostic`,
//! `Def`, `DefKind`, `HirId`, etc.).

// Re-export axiom-lower modules so that `crate::hir`, `crate::error`,
// and `crate::lower` resolve correctly within this crate's source files.
pub mod error {
    pub use axiom_lower::error::*;
}
pub mod hir {
    pub use axiom_lower::hir::*;
}
pub mod lower {
    pub use axiom_lower::lower::*;
}

pub mod desugar;
pub mod intrinsic;
pub mod lang;
pub mod resolve;

// Re-export everything from axiom-lower so that bare type names resolve via
// `crate::TypeName` as well as the module paths above.
pub use axiom_lower::*;

pub use desugar::desugar;
pub use intrinsic::{
    collect_intrinsic_bindings, validate_intrinsic_bindings, IntrinsicBinding, HEAP_ALLOC,
    HEAP_FREE, HEAP_GET, HEAP_SET, KNOWN_INTRINSICS,
};
pub use lang::{
    collect_lang_bindings, resolve_lang_items, LangBinding, LangItems, REQUIRED_LANG_ITEMS,
};
pub use resolve::{build_global_exports, resolve_with_globals, GlobalExports};
