//! HIR type → Ty resolution. Extracted from `collect.rs` to keep both files
//! under the 600-line cap (RUST_CONVENTIONS.md §10).

use super::TypeChecker;
use crate::types::{ErrorSetTy, FnTy, InstanceTy, Ty, TypeParamId};
use resolver::hir_types::*;

impl TypeChecker {
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
            HirTy::Slice(elem) => Ty::HeapBuffer(Box::new(self.resolve_hir_ty(elem))),
            HirTy::ErrorSet(nr) => self.resolve_named_type(nr),
            HirTy::ErrorSetUnion(members) => {
                let mut variant_names = Vec::new();
                for m in members {
                    if let Ty::ErrorSet(es) = self.resolve_hir_ty(m) {
                        for vn in &es.variant_names {
                            if !variant_names.contains(vn) {
                                variant_names.push(vn.clone());
                            }
                        }
                    }
                }
                if variant_names.is_empty() {
                    Ty::Error
                } else {
                    Ty::ErrorSet(ErrorSetTy {
                        name: variant_names.join("||"),
                        def_id: HirId(0),
                        variant_names,
                    })
                }
            }
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

    fn resolve_instance(&self, inst: &resolver::InstanceTy) -> Ty {
        let text = match &inst.name {
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
        let args: Vec<Ty> = inst
            .args
            .iter()
            .map(|arg| self.resolve_hir_ty(arg))
            .collect();
        Ty::Instance(InstanceTy {
            name: text.to_string(),
            def_id: HirId(0),
            args,
        })
    }
}

/// Extract the error set from a function's return type.
/// Returns `Some(ErrorSetTy)` when the return type is `Instance("Result", [_, E])`
/// where `E` is an error set or error set union. Returns `None` for all other types.
pub(super) fn extract_error_set_from_type(ty: &crate::types::Ty) -> Option<ErrorSetTy> {
    match ty {
        crate::types::Ty::Instance(inst) if inst.name == "Result" && inst.args.len() == 2 => {
            match &inst.args[1] {
                crate::types::Ty::ErrorSet(es) => Some(es.clone()),
                crate::types::Ty::Instance(_) => None,
                _ => None,
            }
        }
        _ => None,
    }
}
