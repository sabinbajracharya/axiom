//! Control-flow and structured expression type rules.
//!
//! Block, if/else, match, loop, struct literal, and assign inference.

use super::{helpers, Mutability, TypeChecker, VariantInfo};
use crate::error::TypeDiagnostic;
use crate::types::{FnTy, StructTy, Ty};

use axiom_hir::*;

impl TypeChecker {
    pub(super) fn infer_block(&mut self, block: &Block, expected: &Option<Ty>) -> Ty {
        self.env.push_scope();
        for stmt in &block.stmts {
            self.type_stmt(stmt);
        }
        let ty = if let Some(tail) = &block.tail {
            if let Some(exp) = expected {
                self.check_expr(tail, exp)
            } else {
                self.infer_expr(tail)
            }
        } else {
            Ty::Unit
        };
        self.types.insert(block.id, ty.clone());
        self.env.pop_scope();
        ty
    }

    pub(super) fn check_block(&mut self, block: &Block, expected: &Ty) -> Ty {
        self.infer_block(block, &Some(expected.clone()))
    }

    pub(super) fn infer_if(&mut self, if_expr: &IfExpr) -> Ty {
        let cond_ty = self.infer_expr(&if_expr.condition);
        if !helpers::is_error(&cond_ty) && cond_ty != Ty::Bool {
            self.emit(TypeDiagnostic::ConditionNotBool {
                found: cond_ty.to_string(),
                span: self.span_for(if_expr.id),
            });
        }

        let then_type = self.infer_block(&if_expr.then_branch, &None);

        let ty = if let Some(els) = &if_expr.else_branch {
            let else_type = self.infer_expr(els);
            if helpers::is_error(&then_type) || helpers::is_error(&else_type) {
                if helpers::is_error(&then_type) {
                    then_type
                } else {
                    else_type
                }
            } else if then_type == else_type {
                then_type
            } else {
                self.emit(TypeDiagnostic::IfBranchMismatch {
                    expected: then_type.to_string(),
                    found: else_type.to_string(),
                    span: self.span_for(if_expr.id),
                });
                Ty::Error
            }
        } else {
            if then_type != Ty::Unit && !helpers::is_error(&then_type) {
                self.emit(TypeDiagnostic::IfWithoutElseNotUnit {
                    found: then_type.to_string(),
                    span: self.span_for(if_expr.id),
                });
            }
            Ty::Unit
        };
        self.types.insert(if_expr.id, ty.clone());
        ty
    }

    pub(super) fn infer_match(&mut self, match_expr: &MatchExpr) -> Ty {
        let scrutinee_ty = self.infer_expr(&match_expr.scrutinee);

        let arm_types: Vec<Ty> = match_expr
            .arms
            .iter()
            .map(|arm| {
                self.env.push_scope();
                self.define_pattern_bindings(&arm.pattern, &scrutinee_ty);
                if let Some(guard) = &arm.guard {
                    self.infer_expr(guard);
                }
                let arm_ty = self.infer_expr(&arm.body);
                self.env.pop_scope();
                arm_ty
            })
            .collect();

        if !helpers::is_error(&scrutinee_ty) {
            if let Ty::Enum(enum_ty) = &scrutinee_ty {
                let all_variants: Vec<String> = self
                    .lookup_enum_variants(&enum_ty.name)
                    .map(|vs| vs.iter().map(|v| v.name.clone()).collect())
                    .unwrap_or_default();
                let span = self.span_for(match_expr.id);
                let is_unit_variant = |name: &str| self.is_unit_variant(name);
                for diag in crate::exhaustiveness::check_match_exhaustiveness(
                    &match_expr.arms,
                    &all_variants,
                    &is_unit_variant,
                    span,
                ) {
                    self.emit(diag);
                }
            }
        }

        let ty = if arm_types.is_empty() {
            Ty::Unit
        } else {
            let first_type = &arm_types[0];
            let mut mismatch = false;
            for (i, arm_ty) in arm_types.iter().enumerate().skip(1) {
                if !helpers::is_error(arm_ty)
                    && !helpers::is_error(first_type)
                    && *arm_ty != *first_type
                {
                    self.emit(TypeDiagnostic::MatchArmTypeMismatch {
                        expected: first_type.to_string(),
                        found: arm_ty.to_string(),
                        arm_index: i,
                        span: self.span_for(match_expr.id),
                    });
                    mismatch = true;
                }
            }
            if mismatch {
                Ty::Error
            } else {
                first_type.clone()
            }
        };
        self.types.insert(match_expr.id, ty.clone());
        ty
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    pub(super) fn assign_error(&mut self, id: HirId) -> Ty {
        self.types.insert(id, Ty::Error);
        Ty::Error
    }

    pub(super) fn emit(&mut self, diag: TypeDiagnostic) {
        self.diagnostics.push(diag);
    }

    // TODO(v1): wire up real spans from the HIR. Currently all diagnostics
    // report `0:0:` because the type checker does not yet track source positions.
    pub(super) fn span_for(&self, _id: HirId) -> axiom_lexer::Span {
        axiom_lexer::Span { lo: 0, hi: 0 }
    }

    // ── Struct field registry ────────────────────────────────────────────

    pub(super) fn register_struct_fields(&mut self, _name: &str, _fields: &[(String, Ty)]) {
        // v0: we look up struct fields from the HIR directly.
    }

    pub(super) fn lookup_struct_fields(&self, name: &str) -> Option<Vec<(String, Ty)>> {
        for item in &self.hir.items {
            if let Item::StructDef(s) = item {
                if s.name == name {
                    let fields: Vec<(String, Ty)> = s
                        .fields
                        .iter()
                        .map(|f| (f.name.clone(), self.resolve_hir_ty(&f.ty)))
                        .collect();
                    return Some(fields);
                }
            }
        }
        None
    }

    pub(super) fn register_enum_variants(
        &mut self,
        _name: &str,
        variants: &[VariantInfo],
        enum_ty: &Ty,
    ) {
        for variant in variants {
            let fn_ty = Ty::Fn(FnTy {
                params: variant.payload.clone(),
                return_type: Box::new(enum_ty.clone()),
            });
            self.env.define(
                variant.name.clone(),
                fn_ty,
                variant.def_id,
                Mutability::Immutable,
            );
        }
    }

    pub(super) fn is_unit_variant(&self, name: &str) -> bool {
        if let Some(info) = self.env.lookup(name) {
            if let Ty::Fn(fn_ty) = &info.ty {
                if fn_ty.params.is_empty() {
                    if let Ty::Enum(_) = *fn_ty.return_type {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(super) fn lookup_enum_variants(&self, name: &str) -> Option<Vec<VariantInfo>> {
        for item in &self.hir.items {
            if let Item::EnumDef(e) = item {
                if e.name == name {
                    let variants: Vec<VariantInfo> = e
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
                        .collect();
                    return Some(variants);
                }
            }
        }
        None
    }

    pub(super) fn infer_loop(&mut self, loop_expr: &LoopExpr) -> Ty {
        let ty = match &loop_expr.kind {
            LoopKind::Infinite(body) => {
                self.infer_block(body, &None);
                Ty::Unit
            }
            LoopKind::Conditional { condition, body } => {
                let cond_ty = self.infer_expr(condition);
                if !helpers::is_error(&cond_ty) && cond_ty != Ty::Bool {
                    self.emit(TypeDiagnostic::ConditionNotBool {
                        found: cond_ty.to_string(),
                        span: self.span_for(loop_expr.id),
                    });
                }
                self.infer_block(body, &None);
                Ty::Unit
            }
            LoopKind::Iterator {
                binding,
                binding_id,
                iterable,
                body,
            } => {
                let iterable_ty = self.infer_expr(iterable);
                let binding_ty = if helpers::is_error(&iterable_ty) {
                    Ty::Error
                } else {
                    self.emit(TypeDiagnostic::NotYetSupported {
                        feature: "iterator loops".to_string(),
                        span: self.span_for(loop_expr.id),
                    });
                    Ty::Error
                };
                self.env.define(
                    binding.clone(),
                    binding_ty,
                    *binding_id,
                    Mutability::Immutable,
                );
                self.infer_block(body, &None);
                Ty::Unit
            }
        };
        self.types.insert(loop_expr.id, ty.clone());
        ty
    }

    pub(super) fn infer_struct_lit(&mut self, sl: &StructLitExpr) -> Ty {
        let struct_ty = match &sl.type_name {
            NameRef::Resolved(r) => {
                if let Some(info) = self.env.lookup(&r.text) {
                    if let Ty::Struct(s) = &info.ty {
                        s.clone()
                    } else {
                        self.emit(TypeDiagnostic::TypeMismatch {
                            expected: "struct".to_string(),
                            found: info.ty.to_string(),
                            span: self.span_for(sl.id),
                        });
                        return self.assign_error(sl.id);
                    }
                } else {
                    self.emit(TypeDiagnostic::UndefinedType {
                        name: r.text.clone(),
                        span: self.span_for(sl.id),
                    });
                    return self.assign_error(sl.id);
                }
            }
            NameRef::Unresolved(_) => {
                return self.assign_error(sl.id);
            }
        };

        for field in &sl.fields {
            self.infer_expr(&field.value);
        }

        let ty = self.check_struct_fields(sl, &struct_ty);
        self.types.insert(sl.id, ty.clone());
        ty
    }

    fn check_struct_fields(&mut self, sl: &StructLitExpr, struct_ty: &StructTy) -> Ty {
        let fields = self.lookup_struct_fields(&struct_ty.name);
        match fields {
            Some(expected_fields) => {
                let provided_names: Vec<&str> = sl.fields.iter().map(|f| f.name.as_str()).collect();
                for (name, _) in &expected_fields {
                    if !provided_names.contains(&name.as_str()) {
                        self.emit(TypeDiagnostic::StructMissingField {
                            name: struct_ty.name.clone(),
                            field: name.clone(),
                            span: self.span_for(sl.id),
                        });
                    }
                }
                for field in &sl.fields {
                    if !expected_fields.iter().any(|(n, _)| *n == field.name) {
                        self.emit(TypeDiagnostic::StructUnknownField {
                            name: struct_ty.name.clone(),
                            field: field.name.clone(),
                            span: self.span_for(sl.id),
                        });
                    }
                }
                if sl.fields.len() != expected_fields.len() {
                    self.emit(TypeDiagnostic::StructFieldCountMismatch {
                        name: struct_ty.name.clone(),
                        expected: expected_fields.len(),
                        found: sl.fields.len(),
                        span: self.span_for(sl.id),
                    });
                }
                for field in &sl.fields {
                    if let Some((_, expected_ty)) =
                        expected_fields.iter().find(|(n, _)| *n == field.name)
                    {
                        let value_ty = self
                            .types
                            .get(&field.value.id())
                            .cloned()
                            .unwrap_or(Ty::Error);
                        if !helpers::is_error(&value_ty)
                            && !helpers::is_error(expected_ty)
                            && value_ty != *expected_ty
                        {
                            self.emit(TypeDiagnostic::TypeMismatch {
                                expected: expected_ty.to_string(),
                                found: value_ty.to_string(),
                                span: self.span_for(field.value.id()),
                            });
                        }
                    }
                }
                Ty::Struct(struct_ty.clone())
            }
            None => Ty::Error,
        }
    }

    pub(super) fn infer_assign(&mut self, assign: &AssignExpr) -> Ty {
        let value_ty = self.infer_expr(&assign.value);

        let ty = match &assign.target {
            AssignTarget::Name(nr) => {
                match nr {
                    NameRef::Resolved(r) => {
                        let lookup = self
                            .env
                            .lookup(&r.text)
                            .map(|info| (info.ty.clone(), info.mutability));
                        let is_mutable = self
                            .mutability
                            .get(&r.def_id)
                            .map(|m| *m == Mutability::Mutable)
                            .unwrap_or(false);
                        if let Some((binding_ty, _)) = &lookup {
                            if !is_mutable {
                                self.emit(TypeDiagnostic::AssignToImmutable {
                                    name: r.text.clone(),
                                    span: self.span_for(assign.id),
                                });
                            }
                            if !helpers::is_error(&value_ty)
                                && !helpers::is_error(binding_ty)
                                && value_ty != *binding_ty
                            {
                                self.emit(TypeDiagnostic::TypeMismatch {
                                    expected: binding_ty.to_string(),
                                    found: value_ty.to_string(),
                                    span: self.span_for(assign.id),
                                });
                            }
                        } else {
                            self.emit(TypeDiagnostic::UndefinedType {
                                name: r.text.clone(),
                                span: self.span_for(assign.id),
                            });
                        }
                    }
                    NameRef::Unresolved(_) => {
                        // HIR already diagnosed.
                    }
                }
                Ty::Unit
            }
            AssignTarget::Field { receiver, field: _ } => {
                self.infer_expr(receiver);
                Ty::Unit
            }
            AssignTarget::Index { base, index } => {
                self.infer_expr(base);
                self.infer_expr(index);
                Ty::Unit
            }
        };
        self.types.insert(assign.id, ty.clone());
        ty
    }
}
