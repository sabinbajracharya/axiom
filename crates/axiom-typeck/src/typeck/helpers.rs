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

/// Look up a compiler-intrinsic function by name. Returns `None` for unknown
/// names.
///
/// `print`/`println` are real functions in `stdlib/io.ax`. The paths that
/// *prepend* the stdlib (`with_stdlib` — single-file check, VM) resolve them
/// there and never reach this fallback. But the path that loads the stdlib as
/// modules and expects a **prelude** to auto-import `print`/`println` has no
/// prelude yet (it is a deferred prerequisite — see `modules-design.md` Phase 4
/// and `extern-buffers-and-path-unification.md`). Until the prelude lands, these
/// entries stand in for it so bare `print`/`println` resolve. **Remove once the
/// prelude exists.**
pub(super) fn builtin_fn(name: &str) -> Option<Ty> {
    match name {
        // Interim prelude stand-in (see above). print/println accept any type —
        // a type parameter lets the unifier bind it at each call site.
        "print" | "println" => Some(Ty::Fn(crate::types::FnTy {
            params: vec![Ty::TypeParam(crate::types::TypeParamId {
                name: "T".to_string(),
                index: 0,
                // Sentinel HirId — builtins have no real definition site.
                def_id: axiom_hir::HirId(usize::MAX),
            })],
            return_type: Box::new(Ty::Unit),
        })),
        // `todo()` — stub for unimplemented functions. Returns Ty::Error which
        // suppresses type-mismatch diagnostics (both sides checked for is_error).
        "todo" => Some(Ty::Fn(crate::types::FnTy {
            params: vec![],
            return_type: Box::new(Ty::Error),
        })),
        _ => None,
    }
}
