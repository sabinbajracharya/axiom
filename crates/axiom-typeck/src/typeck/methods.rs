//! Method call, field access, index, and list literal type inference.

use super::{helpers, TypeChecker};
use crate::error::TypeDiagnostic;
use crate::types::{InstanceTy, Ty, TypeParamId};

use super::unify::Substitution;
use axiom_hir::*;

/// A type-parameter scope: each parameter's name, defining `HirId`, and trait
/// bounds — the shape [`TypeChecker::current_type_params`] expects.
type TypeParamScope = Vec<(String, HirId, Vec<String>)>;

impl TypeChecker {
    pub(super) fn infer_method_call(&mut self, mc: &MethodCallExpr) -> Ty {
        let receiver_ty = self.infer_expr(&mc.receiver);
        let arg_types: Vec<Ty> = mc.args.iter().map(|a| self.infer_expr(a)).collect();

        let ty = if helpers::is_error(&receiver_ty) {
            Ty::Error
        } else if let Ty::TypeParam(tp) = &receiver_ty {
            // Calling a trait method on a bounded type parameter (`key.hash()`
            // where `key: K, K: Hashable`). Resolve it through the parameter's
            // declared bounds; the concrete impl is dispatched after
            // monomorphization (IR substitutes the receiver type).
            self.infer_type_param_method(tp, mc, &arg_types)
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

        // Resolve the signature in the impl's own type-param scope so a return
        // type like `List<T>` comes back keyed by the impl's parameter.
        let saved = std::mem::replace(&mut self.current_type_params, type_params);
        let params: Vec<Ty> = fn_def
            .params
            .iter()
            .map(|p| {
                p.ty.as_ref()
                    .map(|t| self.resolve_hir_ty(t))
                    .unwrap_or(Ty::Error)
            })
            .collect();
        let return_type = fn_def
            .return_type
            .as_ref()
            .map(|t| self.resolve_hir_ty(t))
            .unwrap_or(Ty::Unit);
        self.current_type_params = saved;

        let fn_ty = crate::types::FnTy {
            params,
            return_type: Box::new(return_type),
        };
        Some(self.check_call_args(call, &fn_ty, arg_types))
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

    /// Find the **read** subscript for the given type and index-param count.
    fn find_impl_subscript(
        &self,
        type_name: &str,
        receiver_ty: &Ty,
        index_count: usize,
    ) -> Option<(&SubscriptDef, Substitution)> {
        for info in &self.impl_table {
            if info.type_name == type_name {
                if let Some(sub) = info
                    .subscripts
                    .iter()
                    .find(|s| !s.is_setter && index_param_count(s) == index_count)
                {
                    let subst = self.build_impl_subst(info, receiver_ty);
                    return Some((sub, subst));
                }
            }
        }
        None
    }

    /// Find the **write** subscript for the given type and index-param count.
    fn find_impl_write_subscript(
        &self,
        type_name: &str,
        receiver_ty: &Ty,
        index_count: usize,
    ) -> Option<(&SubscriptDef, Substitution)> {
        for info in &self.impl_table {
            if info.type_name == type_name {
                if let Some(sub) = info
                    .subscripts
                    .iter()
                    .find(|s| s.is_setter && index_param_count(s) == index_count)
                {
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
            def_id: self.lang_def_id_for_type(&info.type_name),
            args,
        })
    }

    /// The real stdlib `DefId` for a type that is a compiler lang item (today:
    /// `List`), or the `HirId(0)` placeholder for ordinary types — whose
    /// `Instance.def_id` is never read downstream, so the placeholder is inert.
    /// This kills the `HirId(0)` lie for the list type specifically (C2, §3.2).
    fn lang_def_id_for_type(&self, type_name: &str) -> HirId {
        if type_name == axiom_hir::lang::LIST {
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

    pub(super) fn infer_index(&mut self, index: &IndexExpr) -> Ty {
        let base_ty = self.infer_expr(&index.base);
        for idx in &index.indices {
            self.infer_expr(idx);
        }

        // A heap buffer `[T]` (the P4 storage primitive) indexes by `Int`,
        // yielding `T` directly — no library subscript needed.
        if let Ty::HeapBuffer(elem) = &base_ty {
            let ty = (**elem).clone();
            self.types.insert(index.id, ty.clone());
            return ty;
        }

        // Extract the type name for subscript lookup.
        let type_name = Self::type_name_from_ty(&base_ty);

        // Try subscript lookup first (library-defined indexing).
        if let Some(ref name) = type_name {
            if let Some((sub, subst)) =
                self.find_impl_subscript(name, &base_ty, index.indices.len())
            {
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

    /// Type-check an indexed-place assignment target `base[index] = value`.
    ///
    /// A raw `[T]` heap buffer accepts a `T`. A library collection or user
    /// struct must expose a **write** subscript (`subscript(index, value)`, no
    /// return type — `docs/mutable-subscript-design.md` §4.2); the assigned
    /// value is checked against that setter's value-parameter type. A base with
    /// no writable subscript is a hard error.
    pub(super) fn check_index_assign(
        &mut self,
        base: &Expr,
        indices: &[Expr],
        value_ty: &Ty,
        assign_id: HirId,
    ) {
        let base_ty = self.infer_expr(base);
        for idx in indices {
            self.infer_expr(idx);
        }

        if let Ty::HeapBuffer(elem) = &base_ty {
            if !helpers::is_error(value_ty) && !helpers::is_error(elem) && value_ty != elem.as_ref()
            {
                self.emit(TypeDiagnostic::TypeMismatch {
                    expected: elem.to_string(),
                    found: value_ty.to_string(),
                    span: self.span_for(assign_id),
                });
            }
            return;
        }

        if helpers::is_error(&base_ty) {
            return;
        }

        let Some(name) = Self::type_name_from_ty(&base_ty) else {
            self.emit(TypeDiagnostic::NoWritableSubscript {
                ty: base_ty.to_string(),
                span: self.span_for(assign_id),
            });
            return;
        };

        let Some((sub, subst)) = self.find_impl_write_subscript(&name, &base_ty, indices.len())
        else {
            self.emit(TypeDiagnostic::NoWritableSubscript {
                ty: name,
                span: self.span_for(assign_id),
            });
            return;
        };

        // The last setter parameter is the assigned value; check the value type
        // against it (after substituting the impl's type parameters).
        if let Some(value_param) = sub.params.last() {
            if let Some(param_ty) = &value_param.ty {
                let expected = Self::substitute(&self.resolve_hir_ty(param_ty), &subst);
                if !helpers::is_error(value_ty)
                    && !helpers::is_error(&expected)
                    && *value_ty != expected
                {
                    self.emit(TypeDiagnostic::TypeMismatch {
                        expected: expected.to_string(),
                        found: value_ty.to_string(),
                        span: self.span_for(assign_id),
                    });
                }
            }
        }
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
            name: axiom_hir::lang::LIST.to_string(),
            def_id: self.lang_items.list.unwrap_or(HirId(0)),
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

/// Number of index params for a subscript (total params minus self, minus value
/// param for setters).
fn index_param_count(s: &SubscriptDef) -> usize {
    let total = s.params.len();
    if s.is_setter {
        total.saturating_sub(2) // self + value
    } else {
        total.saturating_sub(1) // self
    }
}
