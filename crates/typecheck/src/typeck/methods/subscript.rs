//! Subscript (index) access resolution: `x[i]` for library collections and
//! user types with `subscript` declarations, plus the write-side
//! `x[i] = value` against setters.

use super::super::unify::Substitution;
use super::impl_type_param_scope;
use super::TypeParamScope;
use super::{helpers, TypeChecker};
use crate::error::TypeDiagnostic;
use crate::types::Ty;
use resolver::*;

impl TypeChecker {
    /// Walk the impl table to find a subscript matching `name` with the given
    /// number of index parameters and setter flag. Returns the resolved
    /// subscript definition, its substitution, and type-parameter scope.
    fn find_impl_subscript(
        &self,
        name: &str,
        base_ty: &Ty,
        num_indices: usize,
        is_setter: bool,
    ) -> Option<(SubscriptDef, Substitution, TypeParamScope)> {
        for info in &self.impl_table {
            if info.type_name != name {
                continue;
            }
            if let Some(s) = info
                .subscripts
                .iter()
                .find(|s| s.is_setter == is_setter && index_param_count(s) == num_indices)
            {
                let subst = self.build_impl_subst(info, base_ty);
                let scope = impl_type_param_scope(info);
                return Some((s.clone(), subst, scope));
            }
        }
        None
    }

    pub(crate) fn infer_index(&mut self, index: &IndexExpr) -> Ty {
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
            if let Some((sub, subst, scope)) =
                self.find_impl_subscript(name, &base_ty, index.indices.len(), false)
            {
                let ty = self.with_type_params(scope, |s| {
                    sub.return_type
                        .as_ref()
                        .map(|t| {
                            let resolved = s.resolve_hir_ty(t);
                            Self::substitute(&resolved, &subst)
                        })
                        .unwrap_or(Ty::Unit)
                });
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
    pub(crate) fn check_index_assign(
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

        // Walk the impl table manually — same reason as infer_index.
        let Some((sub_def, subst, scope)) =
            self.find_impl_subscript(&name, &base_ty, indices.len(), true)
        else {
            self.emit(TypeDiagnostic::NoWritableSubscript {
                ty: name,
                span: self.span_for(assign_id),
            });
            return;
        };

        self.with_type_params(scope, |s| {
            if let Some(value_param) = sub_def.params.last() {
                if let Some(param_ty) = &value_param.ty {
                    let expected = Self::substitute(&s.resolve_hir_ty(param_ty), &subst);
                    if !helpers::is_error(value_ty)
                        && !helpers::is_error(&expected)
                        && *value_ty != expected
                    {
                        s.emit(TypeDiagnostic::TypeMismatch {
                            expected: expected.to_string(),
                            found: value_ty.to_string(),
                            span: s.span_for(assign_id),
                        });
                    }
                }
            }
        });
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
