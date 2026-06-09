//! Method call, field access, index, and list literal type inference.

mod subscript;

use super::unify::Substitution;
use super::{helpers, TypeChecker, TypeParamScope};
use crate::error::TypeDiagnostic;
use crate::types::{InstanceTy, Ty, TypeParamId};

use resolver::*;

/// Build a TypeParamScope from an ImplInfo's type parameters. Bounds are
/// omitted — the scope is only used for name/def_id resolution in
/// `resolve_hir_ty`; bound checking is handled separately by the typeck.
pub(super) fn impl_type_param_scope(info: &super::ImplInfo) -> TypeParamScope {
    info.type_params
        .iter()
        .map(|(name, def_id)| (name.clone(), *def_id, Vec::new()))
        .collect()
}

/// A resolved method with its type-parameter scope.
pub(super) struct ResolvedMethod<'a> {
    pub(super) fn_def: &'a FnDef,
    pub(super) scope: TypeParamScope,
}

impl TypeChecker {
    pub(super) fn infer_method_call(&mut self, mc: &MethodCallExpr) -> Ty {
        let receiver_ty = self.infer_expr(&mc.receiver);
        let arg_types: Vec<Ty> = mc.args.iter().map(|a| self.infer_expr(a)).collect();

        let ty = if helpers::is_error(&receiver_ty) {
            Ty::Error
        } else if let Ty::TypeParam(tp) = &receiver_ty {
            self.infer_type_param_method(tp, mc, &arg_types)
        } else {
            self.infer_concrete_method_call(mc, &receiver_ty, &arg_types)
        };
        self.types.insert(mc.id, ty.clone());
        ty
    }

    fn infer_concrete_method_call(
        &mut self,
        mc: &MethodCallExpr,
        receiver_ty: &Ty,
        arg_types: &[Ty],
    ) -> Ty {
        let receiver_name = match receiver_ty {
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
                let method_info = self.find_impl_method(&name, &mc.method, receiver_ty);
                match method_info {
                    Some((fn_def, impl_subst, scope)) => {
                        // Verify the callee's DefId when desugar has supplied one
                        // (e.g. `push` via lang item). If the resolved method's
                        // DefId doesn't match, the stdlib has drifted.
                        if let Some(expected) = mc.callee_def {
                            if fn_def.id != expected {
                                self.emit(TypeDiagnostic::TypeMismatch {
                                    expected: format!("method with DefId {expected}"),
                                    found: format!("resolved to {}", fn_def.name),
                                    span: self.span_for(mc.id),
                                });
                                self.types.insert(mc.id, Ty::Error);
                                return Ty::Error;
                            }
                        }
                        let mut subst = impl_subst;
                        self.bind_instance_type_params(&fn_def, receiver_ty, &mut subst);
                        let resolved = ResolvedMethod {
                            fn_def: &fn_def,
                            scope,
                        };
                        let ret = self.check_method_call(mc, &resolved, arg_types, &mut subst);
                        self.flowback_receiver_type(mc, receiver_ty, &subst);
                        ret
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
    }

    /// Bind the type parameters of an instance type (like `List<Int>`) to the
    /// fn-level type params declared in the method definition.
    fn bind_instance_type_params(
        &self,
        fn_def: &FnDef,
        receiver_ty: &Ty,
        subst: &mut Substitution,
    ) {
        if let Ty::Instance(inst) = receiver_ty {
            for (i, tp) in fn_def.type_params.iter().enumerate() {
                if let Some(arg) = inst.args.get(i) {
                    if !matches!(arg, Ty::TypeParam(p) if p.name == tp.name && p.def_id == tp.id) {
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
        }
    }

    /// After check_method_call unifies type params, flow the bound types
    /// back to the receiver variable in the environment.
    fn flowback_receiver_type(
        &mut self,
        mc: &MethodCallExpr,
        receiver_ty: &Ty,
        subst: &Substitution,
    ) {
        if let Expr::Path(ref path) = *mc.receiver {
            if let NameRef::Resolved(ref r) = path.name_ref {
                let updated = Self::substitute(receiver_ty, subst);
                self.env.update_type(&r.text, updated);
            }
        }
    }

    /// Resolve a method call on a bounded type parameter — e.g. `key.hash()`
    /// where `key: K` and `K: Hashable`. Searches the parameter's declared
    /// bounds (and their supertraits) for a trait that declares the method, and
    /// returns the method's declared return type with `Self` mapped to the type
    /// parameter. The concrete implementation is dispatched after
    /// monomorphization (IR substitutes the receiver's type).
    fn infer_type_param_method(
        &mut self,
        tp: &TypeParamId,
        mc: &MethodCallExpr,
        _arg_types: &[Ty],
    ) -> Ty {
        let bounds = self
            .type_param_bounds
            .get(&tp.def_id)
            .cloned()
            .unwrap_or_default();
        for bound in &bounds {
            if let Some(ret) = self.trait_method_return(bound, &mc.method) {
                // A `Self`-typed result resolves to the receiver's own type.
                return match ret {
                    Ty::TypeParam(ref rt) if rt.name == "Self" => Ty::TypeParam(tp.clone()),
                    other => other,
                };
            }
        }
        self.emit(TypeDiagnostic::UnknownMethod {
            method: mc.method.clone(),
            ty: Ty::TypeParam(tp.clone()).to_string(),
            span: self.span_for(mc.id),
        });
        Ty::Error
    }

    /// The declared return type of `method` on `trait_name` (searching the
    /// trait's required and default methods, then its supertraits). `None` if
    /// the trait does not declare the method.
    fn trait_method_return(&self, trait_name: &str, method: &str) -> Option<Ty> {
        let info = self.trait_registry.get(trait_name)?;
        for m in info
            .required_methods
            .iter()
            .chain(info.default_methods.iter())
        {
            if m.name == method {
                return Some(m.return_type.clone());
            }
        }
        for supertrait in &info.supertraits {
            if let Some(ret) = self.trait_method_return(supertrait, method) {
                return Some(ret);
            }
        }
        None
    }

    /// Resolve a qualified associated-function call (`Type::method(args)`) — a
    /// method with no `self` parameter, such as a constructor (`List::new()`).
    /// Returns its return type (with type parameters inferred from the args, or
    /// left open for the caller's expected type to bind). Returns `None` when
    /// the call is not a qualified associated function, so ordinary call
    /// resolution (enum constructors, module-qualified functions) proceeds.
    pub(super) fn try_assoc_fn_call(&mut self, call: &CallExpr, arg_types: &[Ty]) -> Option<Ty> {
        let type_name = call.qualifier.clone()?;
        let method_name = helpers::call_name(&call.callee);
        let (type_params, fn_def) = self.assoc_fn_def(&type_name, &method_name)?;

        self.with_type_params(type_params, |s| {
            let params: Vec<Ty> = fn_def
                .params
                .iter()
                .map(|p| {
                    p.ty.as_ref()
                        .map(|t| s.resolve_hir_ty(t))
                        .unwrap_or(Ty::Error)
                })
                .collect();
            let return_type = fn_def
                .return_type
                .as_ref()
                .map(|t| s.resolve_hir_ty(t))
                .unwrap_or(Ty::Unit);
            let fn_ty = crate::types::FnTy {
                params,
                return_type: Box::new(return_type),
            };
            Some(s.check_call_args(call, &fn_ty, arg_types))
        })
    }

    /// Find an inherent associated function (a method with no `self` parameter)
    /// named `method_name` on `type_name`. Returns the impl's type-param scope
    /// (name, def_id, bounds) and the function definition.
    fn assoc_fn_def(&self, type_name: &str, method_name: &str) -> Option<(TypeParamScope, FnDef)> {
        for info in &self.impl_table {
            if info.trait_name.is_some() || info.type_name != type_name {
                continue;
            }
            if let Some(m) = info.methods.iter().find(|m| m.name == method_name) {
                if m.params.iter().all(|p| p.name != "self") {
                    let scope = info
                        .type_params
                        .iter()
                        .map(|(name, id)| {
                            let bounds =
                                info.type_param_bounds.get(id).cloned().unwrap_or_default();
                            (name.clone(), *id, bounds)
                        })
                        .collect();
                    return Some((scope, m.clone()));
                }
            }
        }
        None
    }

    /// Find an impl method matching the given type name and method name.
    /// Searches inherent impls first, then trait impls.
    /// For generic impls (e.g., `impl<T> List<T>`), unifies the impl's self
    /// type pattern against the concrete receiver to build a substitution.
    /// Also returns the impl's type-param scope so callers can set
    /// current_type_params before resolving the method's signature.
    pub(super) fn find_impl_method(
        &self,
        type_name: &str,
        method_name: &str,
        receiver_ty: &Ty,
    ) -> Option<(FnDef, Substitution, TypeParamScope)> {
        self.impl_table
            .iter()
            .filter(|info| info.type_name == type_name)
            .find_map(|info| {
                info.methods
                    .iter()
                    .find(|m| m.name == method_name)
                    .map(|m| {
                        let subst = self.build_impl_subst(info, receiver_ty);
                        let scope = impl_type_param_scope(info);
                        (m.clone(), subst, scope)
                    })
            })
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
        Self::unify_instances(receiver_ty, &pattern, &mut subst);
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
            def_id: self.lang_def_id_for_type(&info.type_name),
            args,
        })
    }

    /// The real stdlib `DefId` for a type that is a compiler lang item (today:
    /// `List`), or the `HirId(0)` placeholder for ordinary types — whose
    /// `Instance.def_id` is never read downstream, so the placeholder is inert.
    /// This kills the `HirId(0)` lie for the list type specifically (C2, §3.2).
    fn lang_def_id_for_type(&self, type_name: &str) -> HirId {
        if type_name == resolver::lang::LIST {
            if let Some(id) = self.lang_items.list {
                return id;
            }
        }
        HirId(0)
    }

    /// Check a method call against a resolved FnDef: arity, arg types, return type.
    /// `subst` is the merged type-parameter substitution (impl-level + fn-level).
    pub(super) fn check_method_call(
        &mut self,
        mc: &MethodCallExpr,
        resolved: &ResolvedMethod<'_>,
        arg_types: &[Ty],
        subst: &mut Substitution,
    ) -> Ty {
        let (param_types, return_type) = self.resolve_method_signature(resolved, subst);

        if param_types.len() != arg_types.len() {
            self.emit(TypeDiagnostic::CallArityMismatch {
                name: mc.method.clone(),
                expected: param_types.len(),
                found: arg_types.len(),
                span: self.span_for(mc.id),
            });
            return return_type;
        }

        let has_params = param_types.iter().any(Self::contains_type_param)
            || Self::contains_type_param(&return_type);
        let ty = if has_params {
            for (arg_ty, param_ty) in arg_types.iter().zip(param_types.iter()) {
                if !helpers::is_error(arg_ty) && !helpers::is_error(param_ty) {
                    if let Err(found) = self.unify(arg_ty, param_ty, subst) {
                        self.emit(TypeDiagnostic::TypeMismatch {
                            expected: param_ty.to_string(),
                            found: found.to_string(),
                            span: self.span_for(mc.id),
                        });
                    }
                }
            }
            Self::substitute(&return_type, subst)
        } else {
            for (arg_ty, param_ty) in arg_types.iter().zip(param_types.iter()) {
                if !helpers::is_error(arg_ty) && !helpers::is_error(param_ty) && arg_ty != param_ty
                {
                    self.emit(TypeDiagnostic::TypeMismatch {
                        expected: param_ty.to_string(),
                        found: arg_ty.to_string(),
                        span: self.span_for(mc.id),
                    });
                }
            }
            return_type
        };
        // Check impl-level bounds even when the method signature is fully
        // concrete — the substitution may carry type params from the impl
        // that have trait bounds (e.g. `impl<T: MyBound> Wrapper<T>`).
        self.check_type_bounds(subst, self.span_for(mc.id));
        ty
    }

    fn resolve_method_signature(
        &mut self,
        resolved: &ResolvedMethod<'_>,
        subst: &Substitution,
    ) -> (Vec<Ty>, Ty) {
        self.with_type_params(resolved.scope.clone(), |s| {
            let params: Vec<Ty> = resolved
                .fn_def
                .params
                .iter()
                .filter(|p| p.name != "self")
                .map(|p| {
                    p.ty.as_ref()
                        .map(|t| s.resolve_hir_ty(t))
                        .map(|r| Self::substitute(&r, subst))
                        .unwrap_or(Ty::Error)
                })
                .collect();
            let ret = resolved
                .fn_def
                .return_type
                .as_ref()
                .map(|t| Self::substitute(&s.resolve_hir_ty(t), subst))
                .unwrap_or(Ty::Unit);
            (params, ret)
        })
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
                // A parameterized struct instance (`Box<Int>`, or `Self` inside
                // a generic `impl<T> Box<T>`). Look up the field in the struct's
                // own type-param scope, then substitute the instance's concrete
                // type arguments for the struct's parameters.
                Ty::Instance(inst) => self.infer_instance_field(inst, field, &receiver_ty),
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

    /// Resolve a field access on a parameterized struct instance. The field's
    /// declared type (in the struct's own type-param scope) has the struct's
    /// parameters substituted with the instance's concrete type arguments.
    fn infer_instance_field(
        &mut self,
        inst: &InstanceTy,
        field: &FieldExpr,
        receiver_ty: &Ty,
    ) -> Ty {
        let unknown_field = |checker: &mut Self| {
            checker.emit(TypeDiagnostic::UnknownField {
                field: field.field.clone(),
                ty: receiver_ty.to_string(),
                span: checker.span_for(field.id),
            });
            Ty::Error
        };
        let Some((type_params, fields)) = self.struct_generic_info(&inst.name) else {
            return unknown_field(self);
        };
        let Some((_, field_ty)) = fields.iter().find(|(n, _)| *n == field.field) else {
            return unknown_field(self);
        };
        let mut subst = Substitution::new();
        for (i, tp) in type_params.iter().enumerate() {
            if let Some(arg) = inst.args.get(i) {
                subst.insert(
                    TypeParamId {
                        name: tp.name.clone(),
                        index: i,
                        def_id: tp.id,
                    },
                    arg.clone(),
                );
            }
        }
        Self::substitute(field_ty, &subst)
    }
}
