//! Pass 2 (the check pass): walk fn/trait/impl/subscript bodies and type every
//! expression. Split out of `mod.rs` to keep each file under the size cap; the
//! `TypeChecker` state and its environment live in `mod.rs`.

use super::*;
use crate::error::TypeDiagnostic;
use std::collections::HashMap;

impl TypeChecker {
    pub(super) fn new(
        hir: Hir,
        lang_items: resolver::LangItems,
        def_origins: Vec<(usize, usize, String)>,
    ) -> Self {
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
            current_fn_error_set: None,
            def_origins,
        }
    }

    /// Temporarily set `current_type_params` to `scope`, run `f`, and restore.
    /// Used wherever the typeck resolves generic type parameters from an impl's
    /// scope (method calls, subscript resolution, associated functions).
    pub(super) fn with_type_params<T>(
        &mut self,
        scope: TypeParamScope,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let saved = std::mem::replace(&mut self.current_type_params, scope);
        let result = f(self);
        self.current_type_params = saved;
        result
    }

    /// Return the module name that owns `def_id`, or `None` if `def_origins`
    /// is empty (bare/no-stdlib mode) or the DefId doesn't fall into any range.
    pub(super) fn module_of(&self, def_id: DefId) -> Option<&str> {
        if self.def_origins.is_empty() {
            return None;
        }
        let raw = def_id.0;
        for (start, end, name) in &self.def_origins {
            if raw >= *start && raw < *end {
                return Some(name.as_str());
            }
        }
        None
    }

    pub(super) fn check_pass(&mut self) {
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
    pub(super) fn check_trait_defaults(&mut self) {
        for item in self.hir.items.clone() {
            let Item::TraitDef(t) = item else {
                continue;
            };
            self.register_trait_self(&t);
            let saved_len = self.current_type_params.len();
            for m in &t.methods {
                if let Some(body) = &m.body {
                    // Extend with method-level type params so they're in scope
                    // inside the default body.
                    for mtp in &m.type_params {
                        let bounds: Vec<String> = mtp
                            .bounds
                            .iter()
                            .map(|b| collect::name_text(&b.name))
                            .collect();
                        self.current_type_params
                            .push((mtp.name.clone(), mtp.id, bounds));
                    }
                    self.check_trait_default_body(m, body);
                    self.current_type_params.truncate(saved_len);
                }
            }
            self.current_self_type = None;
            self.current_type_params.clear();
        }
    }

    /// Set up the type-param scope for checking a trait's default bodies: `Self`
    /// is a type parameter (keyed by the trait's `HirId`) bounded by the trait
    /// itself plus its supertraits. Returns the `Self` type.
    pub(super) fn register_trait_self(&mut self, t: &TraitDef) -> crate::types::Ty {
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
    pub(super) fn check_trait_default_body(&mut self, m: &TraitMethod, body: &Block) {
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
    pub(super) fn resolve_impl_self_type(&self, impl_def: &ImplDef) -> crate::types::Ty {
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

    pub(super) fn check_fn_body(&mut self, f: &FnDef) {
        let impl_param_count = self.current_type_params.len();
        self.extend_type_params(&f.type_params);
        self.register_params(&f.params);
        let return_type = f
            .return_type
            .as_ref()
            .map(|t| self.resolve_hir_ty(t))
            .unwrap_or(crate::types::Ty::Unit);
        // Extract the error set from the return type for coercion checks.
        self.current_fn_error_set = ty_resolve::extract_error_set_from_type(&return_type);
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

    /// Extend `current_type_params` with new type params. Emits an error if
    /// a type param shadows an existing one in the outer scope.
    /// Also registers bounds in `type_param_bounds` for use at call sites.
    pub(super) fn extend_type_params(&mut self, type_params: &[resolver::HirTypeParam]) {
        for tp in type_params {
            let bounds: Vec<String> = tp
                .bounds
                .iter()
                .map(|b| collect::name_text(&b.name))
                .collect();
            if self
                .current_type_params
                .iter()
                .any(|(name, _, _)| *name == tp.name)
            {
                self.emit(TypeDiagnostic::DuplicateTypeParam {
                    name: tp.name.clone(),
                    span: self.span_for(tp.id),
                });
            } else {
                self.current_type_params
                    .push((tp.name.clone(), tp.id, bounds.clone()));
            }
            if !bounds.is_empty() {
                self.type_param_bounds.insert(tp.id, bounds);
            }
        }
    }

    /// Register function/subscript parameters in the current scope.
    pub(super) fn register_params(&mut self, params: &[resolver::Param]) {
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

    pub(super) fn check_subscript_body(&mut self, sub: &SubscriptDef) {
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
