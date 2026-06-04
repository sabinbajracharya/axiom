//! Method call, field access, index, and list literal type inference.

use super::{helpers, TypeChecker};
use crate::error::TypeDiagnostic;
use crate::types::{InstanceTy, Ty, TypeParamId};

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
                    let method_info = self.find_impl_method(&name, &mc.method);
                    match method_info {
                        Some((fn_def, _impl_info)) => {
                            self.check_method_call(mc, &fn_def, &arg_types, &receiver_ty)
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
    pub(super) fn find_impl_method(
        &self,
        type_name: &str,
        method_name: &str,
    ) -> Option<(FnDef, &super::ImplInfo)> {
        for info in &self.impl_table {
            if info.trait_name.is_none() && info.type_name == type_name {
                for m in &info.methods {
                    if m.name == method_name {
                        return Some((m.clone(), info));
                    }
                }
            }
        }
        for info in &self.impl_table {
            if info.trait_name.is_some() && info.type_name == type_name {
                for m in &info.methods {
                    if m.name == method_name {
                        return Some((m.clone(), info));
                    }
                }
            }
        }
        None
    }

    /// Check a method call against a resolved FnDef: arity, arg types, return type.
    /// `receiver_ty` is used to substitute type arguments for generic methods on
    /// Instance types (e.g., `List<Int>.push(42)` substitutes `T → Int`).
    pub(super) fn check_method_call(
        &mut self,
        mc: &MethodCallExpr,
        fn_def: &FnDef,
        arg_types: &[Ty],
        receiver_ty: &Ty,
    ) -> Ty {
        let subst = if let Ty::Instance(inst) = receiver_ty {
            let mut s = super::unify::Substitution::new();
            for (i, tp) in fn_def.type_params.iter().enumerate() {
                if let Some(arg) = inst.args.get(i) {
                    let tp_id = TypeParamId {
                        name: tp.name.clone(),
                        index: i,
                        def_id: tp.id,
                    };
                    s.insert(tp_id, arg.clone());
                }
            }
            s
        } else {
            super::unify::Substitution::new()
        };

        let param_types: Vec<Ty> = fn_def
            .params
            .iter()
            .filter(|p| p.name != "self")
            .map(|p| {
                let resolved =
                    p.ty.as_ref()
                        .map(|t| self.resolve_hir_ty(t))
                        .unwrap_or(Ty::Error);
                Self::substitute(&resolved, &subst)
            })
            .collect();
        let return_type = fn_def
            .return_type
            .as_ref()
            .map(|t| {
                let resolved = self.resolve_hir_ty(t);
                Self::substitute(&resolved, &subst)
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
        let index_ty = self.infer_expr(&index.index);
        let ty = match (&base_ty, &index_ty) {
            (Ty::Instance(inst), Ty::Int) if inst.name == "List" => {
                // List<T>[Int] → T
                inst.args.first().cloned().unwrap_or(Ty::Error)
            }
            (Ty::Instance(inst), _) if inst.name == "Map" => {
                // Map<K, V>[K] → V
                if let (Some(_key_ty), Some(val_ty)) = (inst.args.first(), inst.args.get(1)) {
                    val_ty.clone()
                } else {
                    Ty::Error
                }
            }
            _ => {
                if !helpers::is_error(&base_ty) {
                    self.emit(TypeDiagnostic::NotYetSupported {
                        feature: "index expressions".to_string(),
                        span: self.span_for(index.id),
                    });
                }
                Ty::Error
            }
        };
        self.types.insert(index.id, ty.clone());
        ty
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
