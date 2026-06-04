//! Pass 1: Collect item signatures, struct definitions, and enum definitions
//! into the type environment.

use super::{EnumInfo, FieldInfo, Mutability, StructInfo, TypeChecker, VariantInfo};
use crate::types::{EnumTy, FnTy, StructTy, Ty};
use axiom_hir::*;

impl TypeChecker {
    pub(super) fn collect_pass(&mut self) {
        self.collect_struct_defs();
        self.collect_enum_defs();
        self.collect_fn_sigs();
    }

    fn collect_struct_defs(&mut self) {
        let struct_infos: Vec<StructInfo> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::StructDef(s) => Some(StructInfo {
                    name: s.name.clone(),
                    def_id: s.id,
                    fields: s
                        .fields
                        .iter()
                        .map(|f| {
                            let ty = self.resolve_hir_ty(&f.ty);
                            FieldInfo {
                                name: f.name.clone(),
                                ty,
                            }
                        })
                        .collect(),
                }),
                _ => None,
            })
            .collect();

        for info in &struct_infos {
            let field_types: Vec<(String, Ty)> = info
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.ty.clone()))
                .collect();
            self.env.define(
                info.name.clone(),
                Ty::Struct(StructTy {
                    name: info.name.clone(),
                    def_id: info.def_id,
                }),
                info.def_id,
                Mutability::Immutable,
            );
            self.register_struct_fields(&info.name, &field_types);
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
                }
                Item::StructDef(_) | Item::EnumDef(_) => {}
                Item::TraitDef(_) | Item::ImplDef(_) => {
                    // Traits/impls not yet in type checker — will be added in traits phase 2.
                }
            }
        }
    }

    /// Resolve an `HirTy` (the type syntax in the source) to a `Ty` (the
    /// type checker's internal representation). Unresolved names → Ty::Error.
    pub(super) fn resolve_hir_ty(&self, hir_ty: &HirTy) -> Ty {
        match hir_ty {
            HirTy::Named(nr) => {
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
            HirTy::TypeParam(_tp) => {
                // Phase 2: will resolve to Ty::TypeParam.
                // For now, return Error — type params are not yet in the type env.
                Ty::Error
            }
            HirTy::Instance(inst) => {
                // Phase 2: will resolve to Ty::Instance with resolved type args.
                // For now, resolve the base name as a non-generic type.
                let text = match &inst.name {
                    NameRef::Resolved(r) => &r.text,
                    NameRef::Unresolved(u) => &u.text,
                };
                match text.as_str() {
                    "Int" => Ty::Int,
                    "Float" => Ty::Float,
                    "Bool" => Ty::Bool,
                    "String" => Ty::String,
                    "Unit" => Ty::Unit,
                    _ => Ty::Error,
                }
            }
            HirTy::Error => Ty::Error,
        }
    }
}
