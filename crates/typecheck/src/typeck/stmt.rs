//! Statement type rules and pattern binding.

use super::{helpers, Mutability, TypeChecker};
use crate::types::Ty;

use resolver::*;

impl TypeChecker {
    // ── Statement type rules ─────────────────────────────────────────────

    pub(super) fn type_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::ValStmt(s) => self.type_val_stmt(s),
            Stmt::VarStmt(s) => self.type_var_stmt(s),
            Stmt::ExprStmt(s) => self.type_expr_stmt(s),
            Stmt::ReturnStmt(s) => self.type_return_stmt(s),
            Stmt::BreakStmt(s) => self.type_break_stmt(s),
            Stmt::ContinueStmt(s) => self.type_continue_stmt(s),
            Stmt::YieldStmt(s) => self.type_yield_stmt(s),
        }
    }

    fn type_val_stmt(&mut self, s: &ValStmt) {
        let value_ty = self.infer_expr(&s.value);
        let binding_ty = self.binding_ty(&s.ty, value_ty, s.id);
        self.define_pattern(&s.pattern, &binding_ty, Mutability::Immutable);
        self.types.insert(s.id, Ty::Unit);
    }

    fn type_var_stmt(&mut self, s: &VarStmt) {
        let value_ty = self.infer_expr(&s.value);
        let binding_ty = self.binding_ty(&s.ty, value_ty, s.id);
        self.define_pattern(&s.pattern, &binding_ty, Mutability::Mutable);
        self.types.insert(s.id, Ty::Unit);
    }

    fn binding_ty(&mut self, ann: &Option<resolver::HirTy>, value_ty: Ty, id: HirId) -> Ty {
        let Some(ty_ann) = ann else {
            return value_ty;
        };
        let resolved = self.resolve_hir_ty(ty_ann);
        if !helpers::is_error(&value_ty) && !helpers::is_error(&resolved) {
            let mut subst = super::unify::Substitution::new();
            if self.unify(&resolved, &value_ty, &mut subst).is_err() {
                self.emit(crate::error::TypeDiagnostic::TypeMismatch {
                    expected: resolved.to_string(),
                    found: value_ty.to_string(),
                    span: self.span_for(id),
                });
            }
        }
        resolved
    }

    fn type_expr_stmt(&mut self, s: &ExprStmt) {
        self.infer_expr(&s.expr);
        self.types.insert(s.id, Ty::Unit);
    }

    fn type_return_stmt(&mut self, s: &ReturnStmt) {
        if let Some(v) = &s.value {
            self.infer_expr(v);
        }
        self.types.insert(s.id, Ty::Unit);
    }

    fn type_break_stmt(&mut self, s: &BreakStmt) {
        let value_ty = if let Some(v) = &s.value {
            self.infer_expr(v)
        } else {
            Ty::Unit
        };
        // Record break type for loop type inference.
        if let Some(collector) = self.loop_break_types.last_mut() {
            collector.push(value_ty);
        }
        self.types.insert(s.id, Ty::Unit);
    }

    fn type_continue_stmt(&mut self, s: &ContinueStmt) {
        // Continue is equivalent to break with no value (Unit).
        if let Some(collector) = self.loop_break_types.last_mut() {
            collector.push(Ty::Unit);
        }
        self.types.insert(s.id, Ty::Unit);
    }

    pub(super) fn type_yield_stmt(&mut self, s: &YieldStmt) {
        let value_ty = self.infer_expr(&s.value);
        self.types.insert(s.id, value_ty);
    }

    // ── Pattern binding ──────────────────────────────────────────────────

    fn define_pattern(&mut self, pat: &Pattern, ty: &Ty, mutab: Mutability) {
        match pat {
            Pattern::Ident(p) => {
                if self.is_unit_variant(&p.name) {
                    self.types.insert(p.id, ty.clone());
                } else {
                    self.env.define(p.name.clone(), ty.clone(), p.id, mutab);
                    self.types.insert(p.id, ty.clone());
                    self.mutability.insert(p.id, mutab);
                }
            }
            Pattern::Wildcard(id) => {
                self.types.insert(*id, ty.clone());
            }
            Pattern::Literal(lp) => {
                let lit_ty = helpers::infer_lit(&lp.kind);
                self.types.insert(lp.id, lit_ty);
            }
            Pattern::TupleStruct(ts) => {
                self.define_pattern_tuple_struct(ts, ty);
            }
            Pattern::Struct(sp) => {
                self.define_pattern_struct(sp, ty);
            }
            Pattern::Or(op) => {
                for alt in &op.alternatives {
                    self.define_pattern(alt, ty, mutab);
                }
                self.types.insert(op.id, ty.clone());
            }
            Pattern::Range(rp) => {
                self.types.insert(rp.id, ty.clone());
            }
        }
    }

    fn define_pattern_tuple_struct(&mut self, ts: &TupleStructPat, scrutinee_ty: &Ty) {
        // Match on the last path segment so a qualified pattern (`Shape::Circle(r)`)
        // binds its fields just like the bare form — variant names are unqualified.
        let pat_name = match &ts.path {
            NameRef::Resolved(r) => &r.text,
            NameRef::Unresolved(u) => &u.text,
        };
        let pat_variant = pat_name.rsplit("::").next().unwrap_or(pat_name);
        match scrutinee_ty {
            // Plain enum: payload types are concrete already.
            Ty::Enum(enum_ty) => {
                if let Some(variants) = self.lookup_enum_variants(&enum_ty.name) {
                    if let Some(variant) = variants.iter().find(|v| v.name == pat_variant) {
                        for (i, field_pat) in ts.fields.iter().enumerate() {
                            let field_ty = variant.payload.get(i).cloned().unwrap_or(Ty::Error);
                            self.define_pattern(field_pat, &field_ty, Mutability::Immutable);
                        }
                    }
                }
            }
            // Generic enum instance (`Opt<Int>`): substitute the enum's type
            // parameters with the instance's arguments before binding payloads.
            Ty::Instance(inst) => self.define_pattern_enum_instance(ts, inst, pat_variant),
            _ => {}
        }
        self.types.insert(ts.id, scrutinee_ty.clone());
    }

    /// Bind a tuple-variant pattern against a generic enum instance, mapping the
    /// enum's type parameters to the instance's concrete arguments.
    fn define_pattern_enum_instance(
        &mut self,
        ts: &TupleStructPat,
        inst: &crate::types::InstanceTy,
        pat_variant: &str,
    ) {
        let Some((type_params, variants)) = self.enum_generic_info(&inst.name) else {
            return;
        };
        let Some(variant) = variants.into_iter().find(|v| v.name == pat_variant) else {
            return;
        };
        let mut subst = super::unify::Substitution::new();
        for (i, tp) in type_params.iter().enumerate() {
            if let Some(arg) = inst.args.get(i) {
                subst.insert(
                    crate::types::TypeParamId {
                        name: tp.name.clone(),
                        index: i,
                        def_id: tp.id,
                    },
                    arg.clone(),
                );
            }
        }
        for (i, field_pat) in ts.fields.iter().enumerate() {
            let field_ty = variant
                .payload
                .get(i)
                .map(|t| Self::substitute(t, &subst))
                .unwrap_or(Ty::Error);
            self.define_pattern(field_pat, &field_ty, Mutability::Immutable);
        }
    }

    fn define_pattern_struct(&mut self, sp: &StructPat, scrutinee_ty: &Ty) {
        if let Ty::Struct(struct_ty) = scrutinee_ty {
            if let Some(fields) = self.lookup_struct_fields(&struct_ty.name) {
                for field_pat in &sp.fields {
                    if let Some((_, field_ty)) = fields.iter().find(|(n, _)| *n == field_pat.name) {
                        self.define_pattern(&field_pat.pattern, field_ty, Mutability::Immutable);
                    }
                }
            }
        }
        self.types.insert(sp.id, scrutinee_ty.clone());
    }

    pub(super) fn define_pattern_bindings(&mut self, pat: &Pattern, scrutinee_ty: &Ty) {
        self.define_pattern(pat, scrutinee_ty, Mutability::Immutable);
    }
}
