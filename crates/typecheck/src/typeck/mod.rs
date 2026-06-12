//! The type checker: walks the HIR, assigns types to every expression and
//! statement, and collects type diagnostics.
//!
//! Two-pass design (per `docs/typeck-testing.md` §4.4):
//!   Pass 1 — Collect: register fn signatures, struct definitions, and enum
//!     definitions in the type environment. This allows forward references.
//!   Pass 2 — Check: walk fn bodies, type-checking each expression against the
//!     environment.
//!
//! Bidirectional typing (per §4.1):
//!   - `infer(expr) → Ty`: compute the type from subexpressions and the env.
//!   - `check(expr, expected) → Ty`: verify against an expected type.
//!
//! On error, return `Ty::Error` and emit a diagnostic. `Ty::Error` is sticky
//! (does not cascade additional diagnostics from subexpressions).

mod builtin;
mod check_pass;
mod collect;
mod collect_impls;
mod collect_subscripts;
mod control;
mod helpers;
mod infer;
mod methods;
mod stmt;
mod ty_resolve;
mod typeinfo;
mod unify;

use crate::error::{Diagnostic, TypeDiagnostic};
use crate::thir::{Thir, TypeMap};
use crate::types::ErrorSetTy;

use resolver::*;
use std::collections::HashMap;

/// A type-parameter scope: each parameter's name, defining `HirId`, and trait
/// bounds. Trait bounds are stored for bound-checking but are optional for resolution.
pub(super) type TypeParamScope = Vec<(String, HirId, Vec<String>)>;

// ── Public entry point ────────────────────────────────────────────────────────

/// Type-check an HIR, producing a THIR (HIR + type map + diagnostics).
/// The HIR is consumed (moved) — the THIR owns it.
/// The caller must have already run pre-typecheck desugaring.
/// Never panics on user-reachable input. Returns a Thir even if
/// type errors exist; diagnostics are in `thir.diagnostics`.
pub fn check(hir: Hir) -> Thir {
    check_with_lang_items(hir, resolver::LangItems::default(), Vec::new())
}

/// Type-check with a resolved lang-item registry. The caller (driver or bare
/// `check`) is responsible for desugaring catch/else/ListLit before calling this
/// function. The `?` desugaring must be done by the caller after typecheck
/// (needs inferred types).
pub fn check_with_lang_items(
    mut hir: Hir,
    lang_items: resolver::LangItems,
    def_origins: Vec<(usize, usize, String)>,
) -> Thir {
    let hir_diagnostics: Vec<Diagnostic> = hir.diagnostics.drain(..).map(Diagnostic::Hir).collect();
    let mut checker = TypeChecker::new(hir, lang_items, def_origins);
    checker.collect_pass();
    checker.check_pass();
    Thir {
        hir: checker.hir,
        types: checker.types,
        diagnostics: {
            let mut d = hir_diagnostics;
            d.append(&mut checker.diagnostics);
            d
        },
    }
}

// ── The type checker ──────────────────────────────────────────────────────────

struct TypeChecker {
    hir: Hir,
    types: TypeMap,
    diagnostics: Vec<Diagnostic>,
    env: TypeEnv,
    /// Tracks which HirIds correspond to mutable bindings (var, not val).
    mutability: HashMap<HirId, Mutability>,
    /// Stack of break-type collectors, one per enclosing loop.
    /// Each entry collects the types of `break value` expressions within that loop.
    loop_break_types: Vec<Vec<crate::types::Ty>>,
    /// Type parameters of the function currently being collected or checked.
    /// Each entry is (name, def_id, bound_trait_names).
    /// Set before resolving param/return types, cleared after.
    /// Empty = not inside a generic function.
    current_type_params: Vec<(String, HirId, Vec<String>)>,
    /// Registry of trait definitions, keyed by trait name.
    /// Populated during collect_pass.
    trait_registry: HashMap<String, TraitInfo>,
    /// All impl blocks, collected during collect_pass.
    /// Used for method dispatch and completeness checking.
    impl_table: Vec<ImplInfo>,
    /// The `Self` type inside an impl block's method body.
    /// `None` when not inside an impl method.
    current_self_type: Option<crate::types::Ty>,
    /// Trait bounds for each type parameter, keyed by the type param's HirId.
    /// Populated during collect_pass for all generic functions.
    /// Used by bound checking to find the bounds for a callee's type params.
    type_param_bounds: HashMap<HirId, Vec<String>>,
    /// Compiler-required lang items resolved to real stdlib DefIds. Empty in the
    /// no-stdlib test mode. Read when synthesizing list-literal types so they
    /// point at the true `List` def instead of a placeholder (§3.2).
    lang_items: resolver::LangItems,
    /// The error set declared in the current function's return type.
    /// `Some(es)` when the function returns `E!T` (error union), `None` otherwise.
    /// Used for error set coercion checks on `return` statements.
    current_fn_error_set: Option<ErrorSetTy>,
    /// DefId → module mapping: for each module, the (start_id, end_id, module_name)
    /// range covering the DefIds allocated during its lowering. Used by the orphan
    /// rule to determine which module a trait or type definition comes from.
    def_origins: Vec<(usize, usize, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mutability {
    Immutable,
    Mutable,
}

// ── Type environment ──────────────────────────────────────────────────────────

/// The type environment: a stack of scopes mapping names to types.
struct TypeEnv {
    scopes: Vec<Scope>,
}

struct Scope {
    bindings: HashMap<String, BindingInfo>,
}

struct BindingInfo {
    ty: crate::types::Ty,
    _def_id: DefId,
    mutability: Mutability,
}

struct StructInfo {
    name: String,
    def_id: DefId,
    fields: Vec<FieldInfo>,
}

struct FieldInfo {
    name: String,
    ty: crate::types::Ty,
}

struct VariantInfo {
    name: String,
    def_id: DefId,
    payload: Vec<crate::types::Ty>,
}

/// A trait definition, stored in the registry for completeness checking and method dispatch.
/// `def_id` and `default_methods` are used by bound checking (generics phase 2+) and
/// default method inheritance — they are part of the trait infrastructure.
#[derive(Clone)]
#[allow(dead_code)]
struct TraitInfo {
    name: String,
    def_id: HirId,
    required_methods: Vec<TraitMethodInfo>,
    default_methods: Vec<TraitMethodInfo>,
    /// Supertrait names (e.g., Hashable requires Equatable).
    supertraits: Vec<String>,
}

/// Signature of a trait method (used for completeness checking and bound verification).
#[derive(Clone)]
#[allow(dead_code)]
struct TraitMethodInfo {
    name: String,
    params: Vec<crate::types::Ty>,
    return_type: crate::types::Ty,
}

/// An impl block, stored for method dispatch.
struct ImplInfo {
    /// `None` for inherent impls (`impl Circle { ... }`).
    trait_name: Option<String>,
    type_name: String,
    methods: Vec<FnDef>,
    subscripts: Vec<SubscriptDef>,
    /// Type parameters of the impl block (e.g., `T` in `impl<T> List<T>`).
    /// Empty for non-generic impls.
    type_params: Vec<(String, DefId)>,
    /// Bounds for each type param, keyed by HirId. Used for bound checking
    /// at call sites (e.g., ensuring `T: Hashable` when needed).
    #[allow(dead_code)]
    type_param_bounds: HashMap<DefId, Vec<String>>,
}

impl TypeEnv {
    fn new() -> Self {
        TypeEnv {
            scopes: vec![Scope {
                bindings: HashMap::new(),
            }],
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope {
            bindings: HashMap::new(),
        });
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn define(
        &mut self,
        name: String,
        ty: crate::types::Ty,
        def_id: DefId,
        mutability: Mutability,
    ) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(
                name,
                BindingInfo {
                    ty,
                    _def_id: def_id,
                    mutability,
                },
            );
        }
    }

    fn lookup(&self, name: &str) -> Option<&BindingInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.bindings.get(name) {
                return Some(info);
            }
        }
        None
    }

    fn update_type(&mut self, name: &str, new_ty: crate::types::Ty) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.bindings.get_mut(name) {
                info.ty = new_ty;
                return;
            }
        }
        debug_assert!(false, "update_type called for unknown binding: {name}");
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
