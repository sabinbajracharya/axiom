//! The THIR (Typed HIR): the HIR annotated with type information.
//!
//! Per `docs/typeck-testing.md` §3: the THIR does **not** duplicate the HIR's
//! tree. It wraps the HIR and adds a `TypeMap` side table keyed by `HirId`,
//! plus type-check diagnostics. Downstream stages walk the HIR and look up
//! types by ID.

use crate::error::TypeDiagnostic;
use crate::types::Ty;
use axiom_hir::*;
use std::collections::HashMap;

/// The output of type checking: the original HIR + type map + diagnostics.
/// The HIR is consumed (moved) — the THIR owns it.
pub struct Thir {
    /// The HIR we type-checked (consumed, not cloned).
    pub hir: axiom_hir::Hir,
    /// Maps every HirId (expressions, statements, patterns) to its type.
    pub types: TypeMap,
    /// Type-check diagnostics (type mismatches, missing fields, etc.).
    pub diagnostics: Vec<TypeDiagnostic>,
}

/// A HashMap from HirId to Ty. The THIR's core payload.
pub type TypeMap = HashMap<HirId, Ty>;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_type_map_insert_and_lookup() {
        let mut map = TypeMap::new();
        let id = HirId(0);
        map.insert(id, Ty::Int);
        assert_eq!(map.get(&id), Some(&Ty::Int));
    }

    #[test]
    fn test_type_map_error_sentinel() {
        let mut map = TypeMap::new();
        let id = HirId(42);
        map.insert(id, Ty::Error);
        assert_eq!(map.get(&id), Some(&Ty::Error));
    }

    #[test]
    fn test_thir_holds_hir() {
        let hir = axiom_hir::Hir {
            items: vec![],
            diagnostics: vec![],
        };
        let thir = Thir {
            hir,
            types: TypeMap::new(),
            diagnostics: vec![],
        };
        assert!(thir.hir.items.is_empty());
        assert!(thir.types.is_empty());
        assert!(thir.diagnostics.is_empty());
    }
}
