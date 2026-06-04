//! Statement type rules and pattern binding.

use super::{helpers, Mutability, TypeChecker};
use crate::types::Ty;

use axiom_hir::*;

impl TypeChecker {
    // ── Statement type rules ─────────────────────────────────────────────

    pub(super) fn type_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::ValStmt(s) => self.type_val_stmt(s),
            Stmt::VarStmt(s) => self.type_var_stmt(s),
            Stmt::ExprStmt(s) => self.type_expr_stmt(s),
            Stmt::ReturnStmt(s) => self.type_return_stmt(s),
        }
    }

    fn type_val_stmt(&mut self, s: &ValStmt) {
        let value_ty = self.infer_expr(&s.value);
        let binding_ty = if let Some(ty_ann) = &s.ty {
            let resolved = self.resolve_hir_ty(ty_ann);
            if !helpers::is_error(&value_ty)
                && !helpers::is_error(&resolved)
                && value_ty != resolved
            {
                self.emit(crate::error::TypeDiagnostic::TypeMismatch {
                    expected: resolved.to_string(),
                    found: value_ty.to_string(),
                    span: self.span_for(s.id),
                });
            }
            resolved
        } else {
            value_ty
        };
        self.define_pattern(&s.pattern, &binding_ty, Mutability::Immutable);
        self.types.insert(s.id, Ty::Unit);
    }

    fn type_var_stmt(&mut self, s: &VarStmt) {
        let value_ty = self.infer_expr(&s.value);
        let binding_ty = if let Some(ty_ann) = &s.ty {
            let resolved = self.resolve_hir_ty(ty_ann);
            if !helpers::is_error(&value_ty)
                && !helpers::is_error(&resolved)
                && value_ty != resolved
            {
                self.emit(crate::error::TypeDiagnostic::TypeMismatch {
                    expected: resolved.to_string(),
                    found: value_ty.to_string(),
                    span: self.span_for(s.id),
                });
            }
            resolved
        } else {
            value_ty
        };
        self.define_pattern(&s.pattern, &binding_ty, Mutability::Mutable);
        self.types.insert(s.id, Ty::Unit);
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
        if let Ty::Enum(enum_ty) = scrutinee_ty {
            if let Some(variants) = self.lookup_enum_variants(&enum_ty.name) {
                if let Some(variant) = variants.iter().find(|v| match &ts.path {
                    NameRef::Resolved(r) => v.name == r.text,
                    NameRef::Unresolved(u) => v.name == u.text,
                }) {
                    for (i, field_pat) in ts.fields.iter().enumerate() {
                        let field_ty = variant.payload.get(i).cloned().unwrap_or(Ty::Error);
                        self.define_pattern(field_pat, &field_ty, Mutability::Immutable);
                    }
                }
            }
        }
        self.types.insert(ts.id, scrutinee_ty.clone());
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
