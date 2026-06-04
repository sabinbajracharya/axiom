//! Utility functions for the type checker.

use crate::types::Ty;
use axiom_hir::NameRef;

pub(super) fn is_error(ty: &Ty) -> bool {
    matches!(ty, Ty::Error)
}

pub(super) fn is_numeric(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float)
}

pub(super) fn infer_lit(kind: &axiom_hir::LitKind) -> Ty {
    match kind {
        axiom_hir::LitKind::Int(_) => Ty::Int,
        axiom_hir::LitKind::Float(_) => Ty::Float,
        axiom_hir::LitKind::Bool(_) => Ty::Bool,
        axiom_hir::LitKind::String(_) => Ty::String,
        axiom_hir::LitKind::Unit => Ty::Unit,
    }
}

pub(super) fn call_name(name_ref: &NameRef) -> String {
    match name_ref {
        NameRef::Resolved(r) => r.text.clone(),
        NameRef::Unresolved(u) => u.text.clone(),
    }
}
