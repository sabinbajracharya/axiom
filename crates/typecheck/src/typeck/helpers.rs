//! Utility functions for the type checker.

use crate::types::{FnTy, Ty};
use resolver::NameRef;

pub(super) fn is_error(ty: &Ty) -> bool {
    matches!(ty, Ty::Error)
}

pub(super) fn is_numeric(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float)
}

pub(super) fn infer_lit(kind: &resolver::LitKind) -> Ty {
    match kind {
        resolver::LitKind::Int(_) => Ty::Int,
        resolver::LitKind::Float(_) => Ty::Float,
        resolver::LitKind::Bool(_) => Ty::Bool,
        resolver::LitKind::String(_) => Ty::String,
        resolver::LitKind::Unit => Ty::Unit,
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
/// `print`/`println` are **not** here — they are the real `String`-only
/// functions from `stdlib/std/io.ax`, whose signatures the type checker seeds into
/// every path's environment (`collect.rs::inject_prelude_sigs`). The variadic
/// `format` intrinsic is handled at the call site (`infer_call`), not as a
/// `FnTy`. Only `todo` remains a true intrinsic here.
/// See `docs/string-format-and-print-retire.md`.
pub(super) fn builtin_fn(name: &str) -> Option<Ty> {
    match name {
        // `todo()` — stub for unimplemented functions. Returns Ty::Error which
        // suppresses type-mismatch diagnostics (both sides checked for is_error).
        "todo" => Some(Ty::Fn(FnTy {
            params: vec![],
            return_type: Box::new(Ty::Error),
        })),
        _ => None,
    }
}
