//! Method call, field access, index, and list literal type inference.

use super::{helpers, TypeChecker};
use crate::error::TypeDiagnostic;
use crate::types::{InstanceTy, Ty, TypeParamId};

use super::unify::Substitution;
use axiom_hir::*;

impl TypeChecker {
    pub(super) fn infer_method_call(&mut self, mc: &MethodCallExpr) -> Ty {
        let receiver_ty = self.infer_expr(&mc.receiver);
        let arg_types: Vec<Ty> = mc.args.iter().map(|a| self.infer_expr(a)).collect();

        let ty = if helpers::is_error(&receiver_ty) {
            Ty::Error
        } else {
            let receiver_name = match &receiver_ty {
                Ty::Struct(s) => Some(s.name.clone()),
                Ty::Enum(e) => Some(e.name.clone()),
                Ty::Instance(inst) => Some(inst.name.clone()),
                Ty::Int => Some("Int".to_string()),
                Ty::Float => Some("Float".to_string()),
                Ty::Bool => Some("Bool".to_string()),
                Ty::String => Some("String".to_string()),
                _ => None,
            };

            match receiver_name {
                Some(name) => {
                    let method_info = self.find_impl_method(&name, &mc.method, &receiver_ty);
                    match method_info {
                        Some((fn_def, impl_subst)) => {
                            // Merge impl-level and fn-level substitutions.
                            let mut subst = impl_subst;
                            if let Ty::Instance(inst) = &receiver_ty {
                                for (i, tp) in fn_def.type_params.iter().enumerate() {
                                    if let Some(arg) = inst.args.get(i) {
                                        subst
                                            .entry(TypeParamId {
                                                name: tp.name.clone(),
                                                index: i,
                                                def_id: tp.id,
                                            })
                                            .or_insert_with(|| arg.clone());
                                    }
                                }
                            }
                            self.check_method_call(mc, &fn_def, &arg_types, &subst)
                        }
                        None => {
                            self.emit(TypeDiagnostic::UnknownMethod {
                                method: mc.method.clone(),
                                ty: receiver_ty.to_string(),
                                span: self.span_for(mc.id),
                            });
                            Ty::Error
                        }
                    }
                }
                None => {
                    self.emit(TypeDiagnostic::UnknownMethod {
                        method: mc.method.clone(),
                        ty: receiver_ty.to_string(),
                        span: self.span_for(mc.id),
                    });
                    Ty::Error
                }
            }
        };
        self.types.insert(mc.id, ty.clone());
        ty
    }

    /// Find an impl method matching the given type name and method name.
    /// Searches inherent impls first, then trait impls.
    /// For generic impls (e.g., `impl<T> List<T>`), unifies the impl's self
    /// type pattern against the concrete receiver to build a substitution.
    pub(super) fn find_impl_method(
        &self,
        type_name: &str,
        method_name: &str,
        receiver_ty: &Ty,
    ) -> Option<(FnDef, Substitution)> {
        // Inherent impls first, then trait impls.
        for info in &self.impl_table {
            if info.trait_name.is_none() && info.type_name == type_name {
                if let Some(m) = info.methods.iter().find(|m| m.name == method_name) {
                    let subst = self.build_impl_subst(info, receiver_ty);
                    return Some((m.clone(), subst));
                }
            }
        }
        for info in &self.impl_table {
            if info.trait_name.is_some() && info.type_name == type_name {
                if let Some(m) = info.methods.iter().find(|m| m.name == method_name) {
                    let subst = self.build_impl_subst(info, receiver_ty);
                    return Some((m.clone(), subst));
                }
            }
        }
        None
    }

    /// Find a subscript definition for the given type, returning the subscript
    /// and a type-parameter substitution for generic impls.
    fn find_impl_subscript(
        &self,
        type_name: &str,
        receiver_ty: &Ty,
    ) -> Option<(&SubscriptDef, Substitution)> {
        for info in &self.impl_table {
            if info.type_name == type_name {
                if let Some(sub) = info.subscripts.first() {
                    let subst = self.build_impl_subst(info, receiver_ty);
                    return Some((sub, subst));
                }
            }
        }
        None
    }

    /// Build a type-parameter substitution by unifying an impl's self type
    /// pattern with a concrete receiver type. For non-generic impls, returns
    /// an empty substitution.
    fn build_impl_subst(&self, info: &super::ImplInfo, receiver_ty: &Ty) -> Substitution {
        if info.type_params.is_empty() {
            return Substitution::new();
        }
        let pattern = self.build_impl_self_pattern(info);
        let mut subst = Substitution::new();
        // Unify receiver against pattern: extract TypeParam → concrete mappings.
        unify_instances(receiver_ty, &pattern, &mut subst);
        subst
    }

    /// Build the self-type pattern for a generic impl (e.g., `List<T>` for
    /// `impl<T> List<T>`). Type params become `Ty::TypeParam` placeholders.
    fn build_impl_self_pattern(&self, info: &super::ImplInfo) -> Ty {
        let args: Vec<Ty> = info
            .type_params
            .iter()
            .enumerate()
            .map(|(i, (name, def_id))| {
                Ty::TypeParam(TypeParamId {
                    name: name.clone(),
                    index: i,
                    def_id: *def_id,
                })
            })
            .collect();
        Ty::Instance(InstanceTy {
            name: info.type_name.clone(),
            def_id: HirId(0),
            args,
        })
    }

    /// Check a method call against a resolved FnDef: arity, arg types, return type.
    /// `subst` is the merged type-parameter substitution (impl-level + fn-level).
    pub(super) fn check_method_call(
        &mut self,
        mc: &MethodCallExpr,
        fn_def: &FnDef,
        arg_types: &[Ty],
        subst: &Substitution,
    ) -> Ty {
        let param_types: Vec<Ty> = fn_def
            .params
            .iter()
            .filter(|p| p.name != "self")
            .map(|p| {
                let resolved =
                    p.ty.as_ref()
                        .map(|t| self.resolve_hir_ty(t))
                        .unwrap_or(Ty::Error);
                Self::substitute(&resolved, subst)
            })
            .collect();
        let return_type = fn_def
            .return_type
            .as_ref()
            .map(|t| {
                let resolved = self.resolve_hir_ty(t);
                Self::substitute(&resolved, subst)
            })
            .unwrap_or(Ty::Unit);

        if param_types.len() != arg_types.len() {
            self.emit(TypeDiagnostic::CallArityMismatch {
                name: mc.method.clone(),
                expected: param_types.len(),
                found: arg_types.len(),
                span: self.span_for(mc.id),
            });
            return return_type;
        }

        for (arg_ty, param_ty) in arg_types.iter().zip(param_types.iter()) {
            if !helpers::is_error(arg_ty) && !helpers::is_error(param_ty) && arg_ty != param_ty {
                self.emit(TypeDiagnostic::TypeMismatch {
                    expected: param_ty.to_string(),
                    found: arg_ty.to_string(),
                    span: self.span_for(mc.id),
                });
            }
        }

        return_type
    }

    pub(super) fn infer_field(&mut self, field: &FieldExpr) -> Ty {
        let receiver_ty = self.infer_expr(&field.receiver);
        let ty = if helpers::is_error(&receiver_ty) {
            Ty::Error
        } else {
            match &receiver_ty {
                Ty::Struct(s) => {
                    let fields = self.lookup_struct_fields(&s.name);
                    match fields {
                        Some(fields) => {
                            match fields.iter().find(|(name, _)| *name == field.field) {
                                Some((_, field_ty)) => field_ty.clone(),
                                None => {
                                    self.emit(TypeDiagnostic::UnknownField {
                                        field: field.field.clone(),
                                        ty: receiver_ty.to_string(),
                                        span: self.span_for(field.id),
                                    });
                                    Ty::Error
                                }
                            }
                        }
                        None => Ty::Error,
                    }
                }
                _ => {
                    self.emit(TypeDiagnostic::UnknownField {
                        field: field.field.clone(),
                        ty: receiver_ty.to_string(),
                        span: self.span_for(field.id),
                    });
                    Ty::Error
                }
            }
        };
        self.types.insert(field.id, ty.clone());
        ty
    }

    pub(super) fn infer_index(&mut self, index: &IndexExpr) -> Ty {
        let base_ty = self.infer_expr(&index.base);
        let _index_ty = self.infer_expr(&index.index);

        // Extract the type name for subscript lookup.
        let type_name = Self::type_name_from_ty(&base_ty);

        // Try subscript lookup first (library-defined indexing).
        if let Some(ref name) = type_name {
            if let Some((sub, subst)) = self.find_impl_subscript(name, &base_ty) {
                let ty = sub
                    .return_type
                    .as_ref()
                    .map(|t| {
                        let resolved = self.resolve_hir_ty(t);
                        Self::substitute(&resolved, &subst)
                    })
                    .unwrap_or(Ty::Unit);
                self.types.insert(index.id, ty.clone());
                return ty;
            }
        }

        if !helpers::is_error(&base_ty) {
            self.emit(TypeDiagnostic::NotYetSupported {
                feature: "index expressions".to_string(),
                span: self.span_for(index.id),
            });
        }
        self.types.insert(index.id, Ty::Error);
        Ty::Error
    }

    pub(super) fn infer_list_lit(&mut self, list: &ListLitExpr) -> Ty {
        if list.elements.is_empty() {
            self.emit(TypeDiagnostic::NotYetSupported {
                feature: "empty list literals (use type annotation)".to_string(),
                span: self.span_for(list.id),
            });
            self.types.insert(list.id, Ty::Error);
            return Ty::Error;
        }
        let first_ty = self.infer_expr(&list.elements[0]);
        for elem in &list.elements[1..] {
            let elem_ty = self.infer_expr(elem);
            if !helpers::is_error(&elem_ty) && !helpers::is_error(&first_ty) && elem_ty != first_ty
            {
                self.emit(TypeDiagnostic::TypeMismatch {
                    expected: first_ty.to_string(),
                    found: elem_ty.to_string(),
                    span: self.span_for(elem.id()),
                });
            }
        }
        let ty = Ty::Instance(InstanceTy {
            name: "List".to_string(),
            def_id: HirId(0),
            args: vec![first_ty],
        });
        self.types.insert(list.id, ty.clone());
        ty
    }
}

/// Unify two `Instance` types by matching type arguments positionally.
/// `actual` is the concrete type (e.g., `List<Int>`), `expected` may contain
/// `TypeParam` placeholders (e.g., `List<T>`). Records `T → Int` in `subst`.
fn unify_instances(actual: &Ty, expected: &Ty, subst: &mut Substitution) {
    match (actual, expected) {
        (Ty::Instance(a), Ty::Instance(e)) if a.name == e.name => {
            for (at, et) in a.args.iter().zip(e.args.iter()) {
                unify_instances(at, et, subst);
            }
        }
        (_, Ty::TypeParam(tp)) => {
            subst.entry(tp.clone()).or_insert_with(|| actual.clone());
        }
        _ => {}
    }
}
