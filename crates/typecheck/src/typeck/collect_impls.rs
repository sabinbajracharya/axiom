//! Pass 1 (continued): collect trait definitions, impl blocks, and run impl
//! completeness + orphan-rule checks. Split out of `collect.rs` to stay under
//! the file-size cap; shares the `TypeChecker` state defined in `mod.rs`.

use super::collect_subscripts::{
    check_duplicate_subscripts, is_builtin_type_name, name_def_id, name_text,
};
use super::*;
use crate::types::Ty;
use std::collections::HashMap;

impl TypeChecker {
    /// Collect trait definitions into the trait registry.
    pub(super) fn collect_trait_defs(&mut self) {
        for item in &self.hir.items {
            if let Item::TraitDef(trait_def) = item {
                let mut required = Vec::new();
                let mut default = Vec::new();

                for method in &trait_def.methods {
                    // Set type param scope: trait-level params first.
                    let saved_len = self.current_type_params.len();
                    self.current_type_params = trait_def
                        .type_params
                        .iter()
                        .map(|tp| {
                            let bounds = tp.bounds.iter().map(|b| name_text(&b.name)).collect();
                            (tp.name.clone(), tp.id, bounds)
                        })
                        .collect();
                    // Extend with method-level type params.
                    for mtp in &method.type_params {
                        let bounds: Vec<String> =
                            mtp.bounds.iter().map(|b| name_text(&b.name)).collect();
                        self.current_type_params
                            .push((mtp.name.clone(), mtp.id, bounds));
                    }

                    let params: Vec<Ty> = method
                        .params
                        .iter()
                        .map(|p| {
                            p.ty.as_ref()
                                .map(|t| self.resolve_hir_ty(t))
                                .unwrap_or(Ty::Error)
                        })
                        .collect();
                    let return_type = method
                        .return_type
                        .as_ref()
                        .map(|t| self.resolve_hir_ty(t))
                        .unwrap_or(Ty::Unit);

                    // Restore to trait-level params only (next method starts fresh).
                    self.current_type_params.truncate(saved_len);

                    let info = TraitMethodInfo {
                        name: method.name.clone(),
                        params,
                        return_type,
                    };

                    if method.body.is_some() {
                        default.push(info);
                    } else {
                        required.push(info);
                    }
                }

                // Supertrait names from the `trait X: A + B` clause.
                let supertraits: Vec<String> = trait_def
                    .supertraits
                    .iter()
                    .map(|b| name_text(&b.name))
                    .collect();

                self.trait_registry.insert(
                    trait_def.name.clone(),
                    TraitInfo {
                        name: trait_def.name.clone(),
                        def_id: trait_def.id,
                        required_methods: required,
                        default_methods: default,
                        supertraits,
                    },
                );
            }
        }
    }

    /// Collect impl blocks into the impl table and check completeness.
    pub(super) fn collect_impl_defs(&mut self) {
        for item in &self.hir.items.clone() {
            if let Item::ImplDef(impl_def) = item {
                // Resolve the trait name (if any).
                let trait_name = impl_def.trait_name.as_ref().map(name_text);

                // Resolve the type name.
                let type_name_text = name_text(&impl_def.type_name);

                // The impl target must name a known type: a user-defined type in
                // the environment, or a builtin primitive. Primitives are real
                // types and carry trait impls (Deinit/Equatable/… in
                // `core/primitives.ax` and `core/string.ax`).
                let known = self.env.lookup(&type_name_text).is_some()
                    || is_builtin_type_name(&type_name_text);
                if !known {
                    self.emit(TypeDiagnostic::TypeNotFoundForImpl {
                        name: type_name_text.clone(),
                        span: self.span_for(impl_def.id),
                    });
                }

                // If this is a trait impl, verify the trait exists.
                if let Some(ref tn) = trait_name {
                    if !self.trait_registry.contains_key(tn) {
                        self.emit(TypeDiagnostic::TraitNotFound {
                            name: tn.clone(),
                            span: self.span_for(impl_def.id),
                        });
                    }
                }

                // Orphan rule (§3.5): a trait impl is allowed only if the impl
                // block lives in the same module as the trait, or the same
                // module as the type. Skip when `def_origins` is empty (bare
                // mode, everything is in one source file), and skip impls that
                // live inside a stdlib module (the stdlib is a coherent unit).
                if let Some(ref tn) = trait_name {
                    self.check_orphan_rule(tn, &type_name_text, impl_def);
                }

                // Completeness check: every required trait method must be provided.
                if let Some(ref tn) = trait_name {
                    self.check_impl_completeness(tn, &type_name_text, impl_def);
                }

                // Collect the impl's type parameters for generic matching.
                let type_params: Vec<(String, DefId)> = impl_def
                    .type_params
                    .iter()
                    .map(|tp| (tp.name.clone(), tp.id))
                    .collect();
                let mut tp_bounds = HashMap::new();
                for tp in &impl_def.type_params {
                    let bounds: Vec<String> =
                        tp.bounds.iter().map(|b| name_text(&b.name)).collect();
                    if !bounds.is_empty() {
                        tp_bounds.insert(tp.id, bounds);
                    }
                }

                // Method dispatch table: the impl's own methods, plus the
                // trait's default-bodied methods that this impl does *not*
                // override. Including the defaults here lets `find_impl_method`
                // resolve a call like `dog.score()` to the trait default when
                // `Dog` provides no `score` of its own.
                let mut methods = impl_def.methods.clone();
                if let Some(ref tn) = trait_name {
                    let overridden: Vec<String> =
                        impl_def.methods.iter().map(|m| m.name.clone()).collect();
                    methods.extend(self.trait_default_fndefs(tn, &overridden));
                }

                // Duplicate-detection for subscripts (H6 guard).
                check_duplicate_subscripts(&impl_def.subscripts, &type_name_text, self);

                self.impl_table.push(ImplInfo {
                    trait_name,
                    type_name: type_name_text,
                    methods,
                    subscripts: impl_def.subscripts.clone(),
                    type_params,
                    type_param_bounds: tp_bounds,
                });
            }
        }
    }

    /// Emit a `MissingTraitMethod` diagnostic for every required (body-less)
    /// trait method the impl fails to provide. Default-bodied methods are not
    /// required — the impl inherits them (see `trait_default_fndefs`).
    fn check_impl_completeness(&mut self, trait_name: &str, type_name: &str, impl_def: &ImplDef) {
        let Some(trait_info) = self.trait_registry.get(trait_name).cloned() else {
            return;
        };

        // Build a map of trait method name → self convention for comparison.
        let trait_conventions: HashMap<String, String> = self
            .hir
            .items
            .iter()
            .find_map(|item| match item {
                Item::TraitDef(t) if t.name == trait_name => {
                    let mut map = HashMap::new();
                    for m in &t.methods {
                        if let Some(s) = m.params.iter().find(|p| p.name == "self") {
                            map.insert(m.name.clone(), s.convention.to_string());
                        }
                    }
                    Some(map)
                }
                _ => None,
            })
            .unwrap_or_default();

        for required in &trait_info.required_methods {
            let Some(impl_method) = impl_def.methods.iter().find(|m| m.name == required.name)
            else {
                self.emit(TypeDiagnostic::MissingTraitMethod {
                    trait_name: trait_name.to_string(),
                    type_name: type_name.to_string(),
                    method: required.name.clone(),
                    span: self.span_for(impl_def.id),
                });
                continue;
            };
            // Check self-parameter convention matches the trait declaration.
            if let Some(expected_conv) = trait_conventions.get(&required.name) {
                let impl_self = impl_method.params.iter().find(|p| p.name == "self");
                if let Some(s) = impl_self {
                    let found_conv = s.convention.to_string();
                    if *expected_conv != found_conv {
                        self.emit(TypeDiagnostic::SelfConventionMismatch {
                            method: required.name.clone(),
                            expected: expected_conv.clone(),
                            found: found_conv,
                            span: self.span_for(impl_def.id),
                        });
                    }
                }
            }
        }
    }

    /// Enforce the orphan rule (§3.5): a trait impl must live in the same module
    /// as the trait, or the same module as the type. Skipped when `def_origins`
    /// is empty (bare mode) or when the impl lives in a stdlib module.
    fn check_orphan_rule(&mut self, trait_name: &str, type_name: &str, impl_def: &ImplDef) {
        if self.def_origins.is_empty() {
            return;
        }
        let trait_def_id = impl_def.trait_name.as_ref().and_then(name_def_id);
        let type_def_id = name_def_id(&impl_def.type_name);
        let Some(impl_mod) = self.module_of(impl_def.id) else {
            return;
        };
        // The user module has an empty name; stdlib modules are non-empty.
        if !impl_mod.is_empty() {
            return;
        }
        let trait_ok = trait_def_id
            .and_then(|id| self.module_of(id))
            .is_some_and(|m| m == impl_mod);
        // Builtins (Int, Float, …) have no DefId → no module ownership check.
        let type_ok = type_def_id
            .and_then(|id| self.module_of(id))
            .is_some_and(|m| m == impl_mod);
        if !trait_ok && !type_ok {
            self.emit(TypeDiagnostic::OrphanImpl {
                trait_name: trait_name.to_string(),
                type_name: type_name.to_string(),
                span: self.span_for(impl_def.id),
            });
        }
    }

    /// Synthesize `FnDef`s for a trait's default-bodied methods that an impl
    /// does not override. These are added to the impl's dispatch table so a
    /// call to a default method on a concrete receiver resolves like any other
    /// method. Only the signature is consulted at the call site (arity, arg and
    /// return types); the body is type-checked once in `check_trait_defaults`.
    fn trait_default_fndefs(&self, trait_name: &str, overridden: &[String]) -> Vec<FnDef> {
        let Some(trait_def) = self.hir.items.iter().find_map(|item| match item {
            Item::TraitDef(t) if t.name == trait_name => Some(t),
            _ => None,
        }) else {
            return Vec::new();
        };
        trait_def
            .methods
            .iter()
            .filter_map(|m| {
                let body = m.body.clone()?;
                if overridden.iter().any(|o| o == &m.name) {
                    return None;
                }
                Some(FnDef {
                    id: m.id,
                    name: m.name.clone(),
                    module_path: String::new(),
                    visibility: Visibility::Public,
                    type_params: Vec::new(),
                    params: m.params.clone(),
                    return_type: m.return_type.clone(),
                    body,
                    extern_abi: None,
                    lang_tag: None,
                    intrinsic_tag: None,
                })
            })
            .collect()
    }

    // `resolve_hir_ty` / `resolve_named_type` / `resolve_instance` live in
    // `ty_resolve.rs` (extracted to stay under the 600-line cap).
}
