//! Utility functions for the type checker.

use crate::types::{FnTy, Ty, TypeParamId};
use axiom_hir::{HirId, NameRef};

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
        // `HeapBuffer<T>` floor ops (P4) — the growable-storage primitive the
        // `List`/`Map` library is built on. The element type `T` is the same
        // synthetic type parameter across each signature (see `heap_t`).
        //   heap_alloc<T>(count: Int) -> [T]          (T is return-only)
        //   heap_get<T>(buf: [T], index: Int) -> T
        //   heap_set<T>(buf: [T], index: Int, value: T)
        //   heap_free<T>(buf: [T])
        "heap_alloc" => Some(heap_fn(vec![Ty::Int], heap_buf())),
        "heap_get" => Some(heap_fn(vec![heap_buf(), Ty::Int], heap_t())),
        "heap_set" => Some(heap_fn(vec![heap_buf(), Ty::Int, heap_t()], Ty::Unit)),
        "heap_free" => Some(heap_fn(vec![heap_buf()], Ty::Unit)),
        _ => None,
    }
}

/// The synthetic element type parameter `T` shared by all `HeapBuffer` floor
/// ops. A fixed `TypeParamId` so the `[T]` arguments and the `T` results unify
/// to the same parameter within a single call's substitution.
fn heap_t() -> Ty {
    Ty::TypeParam(TypeParamId {
        name: "T".to_string(),
        index: 0,
        def_id: HirId(0),
    })
}

/// `[T]` — a heap buffer of the synthetic element type.
fn heap_buf() -> Ty {
    Ty::HeapBuffer(Box::new(heap_t()))
}

/// Build a `HeapBuffer` floor-op function type from params + return type.
fn heap_fn(params: Vec<Ty>, return_type: Ty) -> Ty {
    Ty::Fn(FnTy {
        params,
        return_type: Box::new(return_type),
    })
}
