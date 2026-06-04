//! Pass 1: Collect item signatures, struct definitions, enum definitions,
//! trait definitions, and impl blocks into the type environment.

use super::{
    EnumInfo, FieldInfo, ImplInfo, Mutability, StructInfo, TraitInfo, TraitMethodInfo, TypeChecker,
    VariantInfo,
};
use crate::error::TypeDiagnostic;
use crate::types::{EnumTy, FnTy, InstanceTy, StructTy, Ty, TypeParamId};
use axiom_hir::*;

impl TypeChecker {
    pub(super) fn collect_pass(&mut self) {
        self.register_builtin_traits();
        self.register_builtin_impls();
        self.register_builtin_types();
        self.register_builtin_methods();
        self.collect_struct_defs();
        self.collect_enum_defs();
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

    fn collect_enum_defs(&mut self) {
        let enum_infos: Vec<EnumInfo> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::EnumDef(e) => Some(EnumInfo {
                    name: e.name.clone(),
                    def_id: e.id,
                    variants: e
                        .variants
                        .iter()
                        .map(|v| {
                            let payload =
                                v.payload.iter().map(|t| self.resolve_hir_ty(t)).collect();
                            VariantInfo {
                                name: v.name.clone(),
                                def_id: v.id,
                                payload,
                            }
                        })
                        .collect(),
                }),
                _ => None,
            })
            .collect();

        for info in &enum_infos {
            let enum_ty = Ty::Enum(EnumTy {
                name: info.name.clone(),
                def_id: info.def_id,
            });
            self.env.define(
                info.name.clone(),
                enum_ty.clone(),
                info.def_id,
                Mutability::Immutable,
            );
            self.register_enum_variants(&info.name, &info.variants, &enum_ty);
        }
    }

    fn collect_fn_sigs(&mut self) {
        for item in &self.hir.items {
            match item {
                Item::FnDef(f) => {
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
                Item::StructDef(_) | Item::EnumDef(_) => {}
                Item::TraitDef(_) | Item::ImplDef(_) => {
                    // Handled by collect_trait_defs / collect_impl_defs.
                }
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
                    // Set type param scope for resolving method signatures.
                    self.current_type_params = trait_def
                        .type_params
                        .iter()
                        .map(|tp| {
                            let bounds = tp.bounds.iter().map(|b| name_text(&b.name)).collect();
                            (tp.name.clone(), tp.id, bounds)
                        })
                        .collect();

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

                    self.current_type_params.clear();

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

                // Collect supertrait names from the trait's own type param bounds.
                let supertraits: Vec<String> = trait_def
                    .type_params
                    .iter()
                    .flat_map(|tp| &tp.bounds)
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

                // Look up the type in the environment.
                let _type_def_id = if let Some(info) = self.env.lookup(&type_name_text) {
                    info._def_id
                } else {
                    self.emit(TypeDiagnostic::TypeNotFoundForImpl {
                        name: type_name_text.clone(),
                        span: self.span_for(impl_def.id),
                    });
                    HirId(0)
                };

                // If this is a trait impl, verify the trait exists.
                if let Some(ref tn) = trait_name {
                    if !self.trait_registry.contains_key(tn) {
                        self.emit(TypeDiagnostic::TraitNotFound {
                            name: tn.clone(),
                            span: self.span_for(impl_def.id),
                        });
                    }
                }

                // Completeness check: every required trait method must be provided.
                if let Some(ref tn) = trait_name {
                    if let Some(trait_info) = self.trait_registry.get(tn).cloned() {
                        for required in &trait_info.required_methods {
                            let has_method =
                                impl_def.methods.iter().any(|m| m.name == required.name);
                            if !has_method {
                                self.emit(TypeDiagnostic::MissingTraitMethod {
                                    trait_name: tn.clone(),
                                    type_name: type_name_text.clone(),
                                    method: required.name.clone(),
                                    span: self.span_for(impl_def.id),
                                });
                            }
                        }
                    }
                }

                self.impl_table.push(ImplInfo {
                    trait_name,
                    type_name: type_name_text,
                    methods: impl_def.methods.clone(),
                });
            }
        }
    }

    /// Resolve an `HirTy` (the type syntax in the source) to a `Ty` (the
    /// type checker's internal representation). Unresolved names → Ty::Error.
    pub(super) fn resolve_hir_ty(&self, hir_ty: &HirTy) -> Ty {
        match hir_ty {
            HirTy::Named(nr) => self.resolve_named_type(nr),
            HirTy::Unit => Ty::Unit,
            HirTy::Tuple(elems) => {
                Ty::Tuple(elems.iter().map(|t| self.resolve_hir_ty(t)).collect())
            }
            HirTy::Fn(f) => {
                let params = f.params.iter().map(|t| self.resolve_hir_ty(t)).collect();
                let return_type = Box::new(self.resolve_hir_ty(&f.return_type));
                Ty::Fn(FnTy {
                    params,
                    return_type,
                })
            }
            HirTy::TypeParam(tp) => {
                // Look up in the current type param scope (set by collect_fn_sigs
                // or check_fn_body).
                if let Some((index, (_, def_id, _bounds))) = self
                    .current_type_params
                    .iter()
                    .enumerate()
                    .find(|(_, (name, _, _))| *name == tp.name)
                {
                    Ty::TypeParam(TypeParamId {
                        name: tp.name.clone(),
                        index,
                        def_id: *def_id,
                    })
                } else {
                    Ty::Error
                }
            }
            HirTy::Instance(inst) => self.resolve_instance(inst),
            HirTy::Error => Ty::Error,
        }
    }

    /// Resolve a named type reference (builtins, Self, or env lookup).
    fn resolve_named_type(&self, nr: &NameRef) -> Ty {
        let text = match nr {
            NameRef::Resolved(r) => &r.text,
            NameRef::Unresolved(u) => &u.text,
        };
        match text.as_str() {
            "Int" => return Ty::Int,
            "Float" => return Ty::Float,
            "Bool" => return Ty::Bool,
            "String" => return Ty::String,
            "Unit" => return Ty::Unit,
            "Self" => {
                if let Some(ref self_ty) = self.current_self_type {
                    return self_ty.clone();
                }
                return Ty::Error;
            }
            _ => {}
        }
        if let Some(info) = self.env.lookup(text) {
            match &info.ty {
                Ty::Struct(s) => Ty::Struct(s.clone()),
                Ty::Enum(e) => Ty::Enum(e.clone()),
                other => other.clone(),
            }
        } else {
            Ty::Error
        }
    }

    fn resolve_instance(&self, inst: &axiom_hir::InstanceTy) -> Ty {
        let text = match &inst.name {
            NameRef::Resolved(r) => &r.text,
            NameRef::Unresolved(u) => &u.text,
        };
        // Builtins don't take type args — resolve to the base type.
        match text.as_str() {
            "Int" => return Ty::Int,
            "Float" => return Ty::Float,
            "Bool" => return Ty::Bool,
            "String" => return Ty::String,
            "Unit" => return Ty::Unit,
            _ => {}
        }
        let args: Vec<Ty> = inst
            .args
            .iter()
            .map(|arg| self.resolve_hir_ty(arg))
            .collect();
        Ty::Instance(InstanceTy {
            name: text.to_string(),
            def_id: HirId(0), // No generic struct instantiation yet.
            args,
        })
    }
}

/// Extract the text from a `NameRef` (resolved or unresolved).
pub(super) fn name_text(nr: &NameRef) -> String {
    match nr {
        NameRef::Resolved(r) => r.text.clone(),
        NameRef::Unresolved(u) => u.text.clone(),
    }
}
