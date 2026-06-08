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
mod collect;
mod control;
mod helpers;
mod infer;
mod methods;
mod stmt;
mod typeinfo;
mod unify;

use crate::error::{Diagnostic, TypeDiagnostic};
use crate::thir::{Thir, TypeMap};

use axiom_hir::*;
use std::collections::HashMap;

/// A type-parameter scope: each parameter's name, defining `HirId`, and trait
/// bounds. Trait bounds are stored for bound-checking but are optional for resolution.
pub(super) type TypeParamScope = Vec<(String, HirId, Vec<String>)>;

// ── Public entry point ────────────────────────────────────────────────────────

/// Type-check an HIR, producing a THIR (HIR + type map + diagnostics).
/// The HIR is consumed (moved) — the THIR owns it.
/// Never panics on user-reachable input. Returns a Thir even if
/// type errors exist; diagnostics are in `thir.diagnostics`.
pub fn check(hir: Hir) -> Thir {
    check_with_lang_items(hir, axiom_hir::LangItems::default())
}

/// Type-check with a resolved lang-item registry. The multi-module driver
/// (`check_modules`) builds the registry from the stdlib and passes it here so
/// list-literal types resolve to the real `List` def; the bare `check` keeps an
/// empty registry for compiler-isolation tests.
///
/// The desugar pass runs here so that both `check` (bare/no-stdlib) and
/// `check_modules` (stdlib-backed) paths go through it — `ListLit` never
/// reaches the type checker.
pub fn check_with_lang_items(mut hir: Hir, lang_items: axiom_hir::LangItems) -> Thir {
    let max_id = crate::hir_max_id(&hir);
    axiom_hir::desugar::desugar(&mut hir, &lang_items, max_id + 1);
    let hir_diagnostics: Vec<Diagnostic> = hir.diagnostics.drain(..).map(Diagnostic::Hir).collect();
    let mut checker = TypeChecker::new(hir, lang_items);
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
    lang_items: axiom_hir::LangItems,
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

impl TypeChecker {
    fn new(hir: Hir, lang_items: axiom_hir::LangItems) -> Self {
        TypeChecker {
            hir,
            types: TypeMap::new(),
            diagnostics: Vec::new(),
            env: TypeEnv::new(),
            mutability: HashMap::new(),
            loop_break_types: Vec::new(),
            current_type_params: Vec::new(),
            trait_registry: HashMap::new(),
            impl_table: Vec::new(),
            current_self_type: None,
            type_param_bounds: HashMap::new(),
            lang_items,
        }
    }

    /// Temporarily set `current_type_params` to `scope`, run `f`, and restore.
    /// Used wherever the typeck resolves generic type parameters from an impl's
    /// scope (method calls, subscript resolution, associated functions).
    fn with_type_params<T>(&mut self, scope: TypeParamScope, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = std::mem::replace(&mut self.current_type_params, scope);
        let result = f(self);
        self.current_type_params = saved;
        result
    }

    fn check_pass(&mut self) {
        self.check_trait_defaults();
        // Clone required: check_fn_body borrows self mutably while iterating.
        for item in self.hir.items.clone() {
            match item {
                Item::FnDef(f) => {
                    self.check_fn_body(&f);
                }
                Item::ImplDef(impl_def) => {
                    // Resolve the Self type for this impl block.
                    let self_ty = self.resolve_impl_self_type(&impl_def);
                    self.current_self_type = Some(self_ty);

                    // Set impl-level type params so T, U, etc. resolve inside
                    // method and subscript bodies. check_fn_body extends (not
                    // clears) these with any method-level type params.
                    self.current_type_params = impl_def
                        .type_params
                        .iter()
                        .map(|tp| {
                            let bounds = tp
                                .bounds
                                .iter()
                                .map(|b| collect::name_text(&b.name))
                                .collect();
                            (tp.name.clone(), tp.id, bounds)
                        })
                        .collect();
                    // Store bounds for bound checking at call sites.
                    for (_, tp_id, bounds) in &self.current_type_params {
                        self.type_param_bounds.insert(*tp_id, bounds.clone());
                    }

                    for method in &impl_def.methods {
                        self.check_fn_body(method);
                    }
                    for sub in &impl_def.subscripts {
                        self.check_subscript_body(sub);
                    }
                    self.current_self_type = None;
                    self.current_type_params.clear();
                }
                _ => {}
            }
        }
    }

    /// Type-check each trait's default-bodied methods once, treating `Self` as
    /// an abstract type parameter bounded by the trait (and its supertraits).
    /// A call like `self.legs()` in the default body resolves through those
    /// bounds and lowers to a runtime-dispatched call — so the single checked
    /// body works for every implementor.
    fn check_trait_defaults(&mut self) {
        for item in self.hir.items.clone() {
            let Item::TraitDef(t) = item else {
                continue;
            };
            self.register_trait_self(&t);
            for m in &t.methods {
                if let Some(body) = &m.body {
                    self.check_trait_default_body(m, body);
                }
            }
            self.current_self_type = None;
            self.current_type_params.clear();
        }
    }

    /// Set up the type-param scope for checking a trait's default bodies: `Self`
    /// is a type parameter (keyed by the trait's `HirId`) bounded by the trait
    /// itself plus its supertraits. Returns the `Self` type.
    fn register_trait_self(&mut self, t: &TraitDef) -> crate::types::Ty {
        let mut bounds = vec![t.name.clone()];
        bounds.extend(t.supertraits.iter().map(|b| collect::name_text(&b.name)));
        self.type_param_bounds.insert(t.id, bounds);
        self.current_type_params = t
            .type_params
            .iter()
            .map(|tp| {
                let tb = tp
                    .bounds
                    .iter()
                    .map(|b| collect::name_text(&b.name))
                    .collect();
                (tp.name.clone(), tp.id, tb)
            })
            .collect();
        let self_ty = crate::types::Ty::TypeParam(crate::types::TypeParamId {
            name: "Self".to_string(),
            index: 0,
            def_id: t.id,
        });
        self.current_self_type = Some(self_ty.clone());
        self_ty
    }

    /// Type-check one trait default method body. `self` binds to the abstract
    /// `Self` type; the resolved return type is recorded at the method's
    /// `HirId` for IR lowering.
    fn check_trait_default_body(&mut self, m: &TraitMethod, body: &Block) {
        self.register_params(&m.params);
        let return_type = m
            .return_type
            .as_ref()
            .map(|t| self.resolve_hir_ty(t))
            .unwrap_or(crate::types::Ty::Unit);
        let body_type = self.check_block(body, &return_type);
        if !helpers::is_error(&body_type)
            && !helpers::is_error(&return_type)
            && body_type != return_type
        {
            self.emit(TypeDiagnostic::ReturnTypeMismatch {
                expected: return_type.to_string(),
                found: body_type.to_string(),
                span: self.span_for(m.id),
            });
        }
        self.types.insert(m.id, return_type);
        self.env.pop_scope();
    }

    /// Resolve the `Self` type for an impl block (the type being implemented).
    /// For generic impls like `impl<T> List<T>`, constructs a `Ty::Instance`
    /// with `Ty::TypeParam` args so that `Self` resolves to `List<T>` inside
    /// method bodies.
    fn resolve_impl_self_type(&self, impl_def: &ImplDef) -> crate::types::Ty {
        let text = match &impl_def.type_name {
            NameRef::Resolved(r) => &r.text,
            NameRef::Unresolved(u) => &u.text,
        };
        if impl_def.type_params.is_empty() {
            // Non-generic: resolve the type name. This handles builtin
            // primitives (Int/Float/Bool/String/Unit — not in the env) as well
            // as user types, so `Self` inside e.g. `impl Hashable for Int` is
            // `Ty::Int` and intrinsic method calls on `self` qualify correctly.
            return self.resolve_hir_ty(&HirTy::Named(impl_def.type_name.clone()));
        }
        // Generic impl: construct Instance with TypeParam args.
        // e.g., `impl<T> List<T>` → Self = Ty::Instance("List", [Ty::TypeParam(T)]).
        let args: Vec<crate::types::Ty> = impl_def
            .type_params
            .iter()
            .enumerate()
            .map(|(i, tp)| {
                crate::types::Ty::TypeParam(crate::types::TypeParamId {
                    name: tp.name.clone(),
                    index: i,
                    def_id: tp.id,
                })
            })
            .collect();
        crate::types::Ty::Instance(crate::types::InstanceTy {
            name: text.to_string(),
            def_id: HirId(0),
            args,
        })
    }

    fn check_fn_body(&mut self, f: &FnDef) {
        let impl_param_count = self.current_type_params.len();
        self.extend_type_params(&f.type_params);
        self.register_params(&f.params);
        let return_type = f
            .return_type
            .as_ref()
            .map(|t| self.resolve_hir_ty(t))
            .unwrap_or(crate::types::Ty::Unit);
        // Extern fns (`extern "C" fn …;`) and intrinsics (`@intrinsic`) have
        // no body we should check — the platform or compiler supplies it.
        // Record the signature and skip the body/return reconciliation.
        if f.extern_abi.is_none() && f.intrinsic_tag.is_none() {
            let body_type = self.check_block(&f.body, &return_type);
            if !helpers::is_error(&body_type)
                && !helpers::is_error(&return_type)
                && body_type != return_type
            {
                self.emit(TypeDiagnostic::ReturnTypeMismatch {
                    expected: return_type.to_string(),
                    found: body_type.to_string(),
                    span: self.span_for(f.id),
                });
                self.types.insert(f.id, crate::types::Ty::Error);
            }
        }
        let fn_ty = crate::types::Ty::Fn(crate::types::FnTy {
            params: f
                .params
                .iter()
                .map(|p| {
                    self.types
                        .get(&p.id)
                        .cloned()
                        .unwrap_or(crate::types::Ty::Error)
                })
                .collect(),
            return_type: Box::new(return_type.clone()),
        });
        self.types.insert(f.id, fn_ty);
        self.env.pop_scope();
        self.current_type_params.truncate(impl_param_count);
    }

    /// Extend `current_type_params` with new type params, skipping duplicates.
    fn extend_type_params(&mut self, type_params: &[axiom_hir::HirTypeParam]) {
        for tp in type_params {
            let bounds = tp
                .bounds
                .iter()
                .map(|b| collect::name_text(&b.name))
                .collect();
            if !self
                .current_type_params
                .iter()
                .any(|(name, _, _)| *name == tp.name)
            {
                self.current_type_params
                    .push((tp.name.clone(), tp.id, bounds));
            }
        }
    }

    /// Register function/subscript parameters in the current scope.
    fn register_params(&mut self, params: &[axiom_hir::Param]) {
        self.env.push_scope();
        for param in params {
            let param_type = if param.name == "self" {
                self.current_self_type
                    .clone()
                    .unwrap_or(crate::types::Ty::Error)
            } else {
                param
                    .ty
                    .as_ref()
                    .map(|t| self.resolve_hir_ty(t))
                    .unwrap_or(crate::types::Ty::Error)
            };
            // `inout`/`sink` parameters are mutable bindings (an `inout` is
            // read-write and written back to the caller; a `sink` is owned);
            // `let` parameters are an immutable borrow.
            let mutability = match param.convention {
                CallingConvention::Inout | CallingConvention::Sink => Mutability::Mutable,
                CallingConvention::Let => Mutability::Immutable,
            };
            self.env
                .define(param.name.clone(), param_type.clone(), param.id, mutability);
            self.types.insert(param.id, param_type);
            self.mutability.insert(param.id, mutability);
        }
    }

    fn check_subscript_body(&mut self, sub: &SubscriptDef) {
        self.env.push_scope();
        // Register parameters. The first `self` param uses the impl's Self type.
        for param in &sub.params {
            let param_type = if param.name == "self" {
                self.current_self_type
                    .clone()
                    .unwrap_or(crate::types::Ty::Error)
            } else {
                param
                    .ty
                    .as_ref()
                    .map(|t| self.resolve_hir_ty(t))
                    .unwrap_or(crate::types::Ty::Error)
            };
            let mutability = match param.convention {
                CallingConvention::Inout | CallingConvention::Sink => Mutability::Mutable,
                CallingConvention::Let => Mutability::Immutable,
            };
            self.env
                .define(param.name.clone(), param_type.clone(), param.id, mutability);
            self.types.insert(param.id, param_type);
            self.mutability.insert(param.id, mutability);
        }
        let return_type = sub
            .return_type
            .as_ref()
            .map(|t| self.resolve_hir_ty(t))
            .unwrap_or(crate::types::Ty::Unit);

        // Type-check the body and record its result type. A subscript body is
        // an ordinary block: a tail expression (`{ self.buf[index] }`) is the
        // result; otherwise a trailing `yield` sets it. Inferring the tail also
        // records types for every body subexpression, which IR lowering needs
        // (e.g. to tell a `[T]` index from a nested subscript call).
        for stmt in &sub.body.stmts {
            self.type_stmt(stmt);
        }
        let body_type = if let Some(tail) = &sub.body.tail {
            self.infer_expr(tail)
        } else {
            sub.body
                .stmts
                .iter()
                .rev()
                .find_map(|stmt| match stmt {
                    Stmt::YieldStmt(s) => self.types.get(&s.id).cloned(),
                    _ => None,
                })
                .unwrap_or(crate::types::Ty::Unit)
        };

        if !helpers::is_error(&body_type)
            && !helpers::is_error(&return_type)
            && body_type != return_type
        {
            self.emit(TypeDiagnostic::ReturnTypeMismatch {
                expected: return_type.to_string(),
                found: body_type.to_string(),
                span: self.span_for(sub.id),
            });
        }
        self.env.pop_scope();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
