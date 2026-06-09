//! Pass 1: Collect item signatures, struct definitions, enum definitions,
//! trait definitions, and impl blocks into the type environment.

use super::{
    FieldInfo, ImplInfo, Mutability, StructInfo, TraitInfo, TraitMethodInfo, TypeChecker,
    VariantInfo,
};
use crate::error::TypeDiagnostic;
use crate::types::{EnumTy, ErrorSetTy, FnTy, InstanceTy, StructTy, Ty};
use parser::ast::AstNode;
use resolver::hir_types::{ErrorSetDef, Item};
use resolver::*;
use std::collections::HashMap;

impl TypeChecker {
    pub(super) fn collect_pass(&mut self) {
        // The core trait declarations (Deinit/Equatable/Hashable/Ord) AND their
        // primitive impls now live in `stdlib/core/*.ax`, collected via
        // collect_trait_defs / collect_impl_defs like any other code. The
        // compiler only registers the irreducible *floor* methods (as_bytes,
        // len, hash_raw, and the List/Map intrinsic stand-ins).
        self.register_builtin_methods();
        self.collect_struct_defs();
        self.register_struct_deinit_impls();
        self.collect_enum_defs();
        self.collect_error_set_defs();
        self.collect_fn_sigs();
        self.collect_trait_defs();
        self.collect_impl_defs();
    }

    fn collect_struct_defs(&mut self) {
        // First pass: resolve field types and collect struct infos.
        // We also track (field_hir_id, resolved_ty) for populating the TypeMap.
        struct StructCollect {
            info: StructInfo,
            field_ids: Vec<(HirId, Ty)>,
        }

        let collected: Vec<StructCollect> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::StructDef(s) => {
                    let mut field_ids = Vec::new();
                    let fields = s
                        .fields
                        .iter()
                        .map(|f| {
                            let ty = self.resolve_hir_ty(&f.ty);
                            field_ids.push((f.id, ty.clone()));
                            FieldInfo {
                                name: f.name.clone(),
                                ty,
                            }
                        })
                        .collect();
                    Some(StructCollect {
                        info: StructInfo {
                            name: s.name.clone(),
                            def_id: s.id,
                            fields,
                        },
                        field_ids,
                    })
                }
                _ => None,
            })
            .collect();

        // Second pass: register in env, TypeMap, and field table.
        for sc in &collected {
            // Populate TypeMap for struct field declarations.
            for (fid, fty) in &sc.field_ids {
                self.types.insert(*fid, fty.clone());
            }
            let field_types: Vec<(String, Ty)> = sc
                .info
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.ty.clone()))
                .collect();
            self.env.define(
                sc.info.name.clone(),
                Ty::Struct(StructTy {
                    name: sc.info.name.clone(),
                    def_id: sc.info.def_id,
                }),
                sc.info.def_id,
                Mutability::Immutable,
            );
            self.register_struct_fields(&sc.info.name, &field_types);
        }
    }

    /// Register `Deinit` auto-impls for every user-defined struct.
    ///
    /// A struct's drop conceptually calls `drop` on each field. All primitive
    /// fields already have Deinit from `register_builtin_impls`; nested structs
    /// get their own Deinit impl from this same pass. The bound-checker resolves
    /// the chain at check time.
    fn register_struct_deinit_impls(&mut self) {
        for item in &self.hir.items {
            if let Item::StructDef(s) = item {
                self.impl_table.push(ImplInfo {
                    trait_name: Some("Deinit".to_string()),
                    type_name: s.name.clone(),
                    methods: vec![],
                    subscripts: vec![],
                    type_params: vec![],
                    type_param_bounds: HashMap::new(),
                });
            }
        }
    }

    fn collect_enum_defs(&mut self) {
        // Clone the enum defs out first so we can resolve each variant's payload
        // in the enum's own type-param scope (which borrows `self` mutably).
        let enum_defs: Vec<EnumDef> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::EnumDef(e) => Some(e.clone()),
                _ => None,
            })
            .collect();

        for e in &enum_defs {
            let saved = std::mem::take(&mut self.current_type_params);
            self.current_type_params = e
                .type_params
                .iter()
                .map(|tp| (tp.name.clone(), tp.id, Vec::new()))
                .collect();
            let variants: Vec<VariantInfo> = e
                .variants
                .iter()
                .map(|v| VariantInfo {
                    name: v.name.clone(),
                    def_id: v.id,
                    payload: v.payload.iter().map(|t| self.resolve_hir_ty(t)).collect(),
                })
                .collect();
            self.current_type_params = saved;

            // A generic enum's values are `Ty::Instance` (carrying the type
            // arguments, like generic structs); a plain enum stays `Ty::Enum`.
            let self_ty = if e.type_params.is_empty() {
                Ty::Enum(EnumTy {
                    name: e.name.clone(),
                    def_id: e.id,
                })
            } else {
                let args = e
                    .type_params
                    .iter()
                    .enumerate()
                    .map(|(i, tp)| {
                        Ty::TypeParam(crate::types::TypeParamId {
                            name: tp.name.clone(),
                            index: i,
                            def_id: tp.id,
                        })
                    })
                    .collect();
                Ty::Instance(InstanceTy {
                    name: e.name.clone(),
                    def_id: e.id,
                    args,
                })
            };
            self.env
                .define(e.name.clone(), self_ty.clone(), e.id, Mutability::Immutable);
            self.register_enum_variants(&e.name, &variants, &self_ty);
        }
    }

    fn collect_error_set_defs(&mut self) {
        let sets: Vec<ErrorSetDef> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::ErrorSetDef(e) => Some(e.clone()),
                _ => None,
            })
            .collect();

        for e in &sets {
            let name = e.name.clone();
            let def_id = resolver::HirId(e.id.0);
            let mut variant_names = Vec::new();

            for v in &e.variants {
                let var_name = v.name.clone();
                let var_def_id = resolver::HirId(v.id.0);
                variant_names.push(var_name.clone());

                let fn_ty = Ty::Fn(FnTy {
                    params: vec![],
                    return_type: Box::new(Ty::ErrorSet(ErrorSetTy {
                        name: name.clone(),
                        def_id,
                        variant_names: vec![],
                    })),
                });
                self.env
                    .define(var_name, fn_ty, var_def_id, Mutability::Immutable);
            }

            self.env.define(
                name.clone(),
                Ty::ErrorSet(ErrorSetTy {
                    name: name.clone(),
                    def_id,
                    variant_names: variant_names.clone(),
                }),
                def_id,
                Mutability::Immutable,
            );
        }
    }

    fn collect_fn_sigs(&mut self) {
        // The prelude's `print`/`println` signatures are seeded first so they are
        // available in every compilation path — including the single-file check
        // path, whose unit does *not* contain the `stdlib/std/io.ax` FnDef bodies
        // (it resolves their names through module exports only). User or
        // in-unit definitions of the same name override these below.
        // See `docs/string-format-and-print-retire.md`.
        self.inject_prelude_sigs();
        // Clone out the in-unit FnDefs first: `register_fn_sig` borrows `self`
        // mutably, which can't coexist with iterating `&self.hir.items`. These
        // are registered after the prelude so an in-unit definition overrides it.
        let fn_defs: Vec<FnDef> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::FnDef(f) => Some(f.clone()),
                _ => None,
            })
            .collect();
        for f in &fn_defs {
            self.register_fn_sig(f);
        }
    }

    /// Register one function's signature in the type environment. Shared by the
    /// in-unit collection pass and the prelude injection.
    fn register_fn_sig(&mut self, f: &FnDef) {
        // Set type param scope so resolve_hir_ty can resolve T, U, etc.
        self.current_type_params = f
            .type_params
            .iter()
            .map(|tp| {
                let bounds = tp.bounds.iter().map(|b| name_text(&b.name)).collect();
                (tp.name.clone(), tp.id, bounds)
            })
            .collect();
        // Store bounds by type param HirId for bound checking at call sites.
        for (_, tp_id, bounds) in &self.current_type_params {
            self.type_param_bounds.insert(*tp_id, bounds.clone());
        }
        let param_types: Vec<Ty> = f
            .params
            .iter()
            .map(|p| {
                p.ty.as_ref()
                    .map(|t| self.resolve_hir_ty(t))
                    .unwrap_or(Ty::Error)
            })
            .collect();
        let return_type = f
            .return_type
            .as_ref()
            .map(|t| self.resolve_hir_ty(t))
            .unwrap_or(Ty::Unit);
        let fn_ty = Ty::Fn(FnTy {
            params: param_types,
            return_type: Box::new(return_type),
        });
        self.env
            .define(f.name.clone(), fn_ty, f.id, Mutability::Immutable);
        self.current_type_params.clear();
    }

    /// Seed the prelude's function signatures (`print`/`println`) into the
    /// environment from the bundled `stdlib/std/io.ax`. This makes them genuinely
    /// the stdlib functions — `String`-only — in every path, retiring the old
    /// hand-written generic stand-in. Signatures only: the bodies are *not*
    /// added to `hir.items`, so THIR dumps stay focused on user code.
    fn inject_prelude_sigs(&mut self) {
        const PRELUDE_IO: &str = include_str!("../../../../crates/stdlib/source/std/io.ax");
        let result = parser::parse(PRELUDE_IO);
        let Some(root) = parser::ast::SourceFile::cast(result.tree) else {
            return;
        };
        let (items, _defs, _diags, _next) = resolver::lower_structural(&root, PRELUDE_IO, 0);
        for item in &items {
            if let Item::FnDef(f) = item {
                self.register_fn_sig(f);
            }
        }
    }

    /// Collect trait definitions into the trait registry.
    fn collect_trait_defs(&mut self) {
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
    fn collect_impl_defs(&mut self) {
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
        for required in &trait_info.required_methods {
            let has_method = impl_def.methods.iter().any(|m| m.name == required.name);
            if !has_method {
                self.emit(TypeDiagnostic::MissingTraitMethod {
                    trait_name: trait_name.to_string(),
                    type_name: type_name.to_string(),
                    method: required.name.clone(),
                    span: self.span_for(impl_def.id),
                });
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
        let trait_def_id = impl_def
            .trait_name
            .as_ref()
            .and_then(name_def_id);
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

// Re-export helpers from collect_subscripts module.
pub(super) use super::collect_subscripts::{
    check_duplicate_subscripts, is_builtin_type_name, name_def_id, name_text,
};
