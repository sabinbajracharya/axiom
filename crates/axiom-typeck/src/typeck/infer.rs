//! Leaf expression type inference: literals, paths, binary/unary ops, calls, fields, indexing.

use super::unify::Substitution;
use super::{helpers, TypeChecker};
use crate::error::TypeDiagnostic;
use crate::types::{FnTy, Ty};

use axiom_hir::*;

impl TypeChecker {
    pub(super) fn infer_expr(&mut self, expr: &Expr) -> Ty {
        match expr {
            Expr::Lit(lit) => {
                let ty = helpers::infer_lit(&lit.kind);
                self.types.insert(lit.id, ty.clone());
                ty
            }
            Expr::Path(path) => self.infer_path(&path.name_ref, path.id),
            Expr::Bin(bin) => self.infer_bin(bin),
            Expr::Unary(unary) => self.infer_unary(unary),
            Expr::Call(call) => self.infer_call(call),
            Expr::MethodCall(mc) => self.infer_method_call(mc),
            Expr::Field(field) => self.infer_field(field),
            Expr::Index(index) => self.infer_index(index),
            Expr::Block(block) => self.infer_block(block, &None),
            Expr::If(if_expr) => self.infer_if(if_expr),
            Expr::Match(match_expr) => self.infer_match(match_expr),
            Expr::Loop(loop_expr) => self.infer_loop(loop_expr),
            Expr::StructLit(sl) => self.infer_struct_lit(sl),
            Expr::Assign(assign) => self.infer_assign(assign),
        }
    }

    pub(super) fn check_expr(&mut self, expr: &Expr, expected: &Ty) -> Ty {
        let inferred = self.infer_expr(expr);
        let result = if helpers::is_error(&inferred) || helpers::is_error(expected) {
            inferred.clone()
        } else if inferred == *expected {
            inferred
        } else {
            self.emit(TypeDiagnostic::TypeMismatch {
                expected: expected.to_string(),
                found: inferred.to_string(),
                span: self.span_for(expr.id()),
            });
            Ty::Error
        };
        self.types.insert(expr.id(), result.clone());
        result
    }

    fn infer_path(&mut self, name_ref: &NameRef, expr_id: HirId) -> Ty {
        let ty = match name_ref {
            NameRef::Resolved(r) => {
                if let Some(info) = self.env.lookup(&r.text) {
                    match &info.ty {
                        Ty::Fn(fn_ty) if fn_ty.params.is_empty() => *fn_ty.return_type.clone(),
                        other => other.clone(),
                    }
                } else {
                    match helpers::builtin_fn(&r.text) {
                        Some(ty) => ty,
                        None => {
                            self.emit(TypeDiagnostic::UndefinedType {
                                name: r.text.clone(),
                                span: self.span_for(expr_id),
                            });
                            Ty::Error
                        }
                    }
                }
            }
            NameRef::Unresolved(_) => Ty::Error,
        };
        self.types.insert(expr_id, ty.clone());
        ty
    }

    fn infer_bin(&mut self, bin: &BinExpr) -> Ty {
        let left_ty = self.infer_expr(&bin.left);
        let right_ty = self.infer_expr(&bin.right);

        let ty = if helpers::is_error(&left_ty) || helpers::is_error(&right_ty) {
            Ty::Error
        } else {
            match bin.op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                    self.infer_arithmetic_bin(bin.op, left_ty, right_ty, bin.id)
                }
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    self.infer_comparison_bin(bin.op, left_ty, right_ty, bin.id)
                }
                BinOp::And | BinOp::Or => self.infer_logical_bin(bin.op, left_ty, right_ty, bin.id),
                BinOp::Shl | BinOp::Shr | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                    self.infer_bitwise_bin(bin.op, left_ty, right_ty, bin.id)
                }
            }
        };
        self.types.insert(bin.id, ty.clone());
        ty
    }

    fn infer_arithmetic_bin(&mut self, op: BinOp, left_ty: Ty, right_ty: Ty, id: HirId) -> Ty {
        if helpers::is_numeric(&left_ty) && left_ty == right_ty {
            left_ty
        } else {
            self.emit(TypeDiagnostic::BinOpMismatch {
                op: op.to_string(),
                left: left_ty.to_string(),
                right: right_ty.to_string(),
                span: self.span_for(id),
            });
            Ty::Error
        }
    }

    fn infer_comparison_bin(&mut self, op: BinOp, left_ty: Ty, right_ty: Ty, id: HirId) -> Ty {
        if left_ty == right_ty {
            Ty::Bool
        } else {
            self.emit(TypeDiagnostic::BinOpMismatch {
                op: op.to_string(),
                left: left_ty.to_string(),
                right: right_ty.to_string(),
                span: self.span_for(id),
            });
            Ty::Error
        }
    }

    fn infer_logical_bin(&mut self, op: BinOp, left_ty: Ty, right_ty: Ty, id: HirId) -> Ty {
        if left_ty == Ty::Bool && right_ty == Ty::Bool {
            Ty::Bool
        } else {
            self.emit(TypeDiagnostic::BinOpMismatch {
                op: op.to_string(),
                left: left_ty.to_string(),
                right: right_ty.to_string(),
                span: self.span_for(id),
            });
            Ty::Error
        }
    }

    fn infer_bitwise_bin(&mut self, op: BinOp, left_ty: Ty, right_ty: Ty, id: HirId) -> Ty {
        if left_ty == Ty::Int && right_ty == Ty::Int {
            Ty::Int
        } else {
            self.emit(TypeDiagnostic::BinOpMismatch {
                op: op.to_string(),
                left: left_ty.to_string(),
                right: right_ty.to_string(),
                span: self.span_for(id),
            });
            Ty::Error
        }
    }

    fn infer_unary(&mut self, unary: &UnaryExpr) -> Ty {
        let operand_ty = self.infer_expr(&unary.operand);
        let ty = if helpers::is_error(&operand_ty) {
            Ty::Error
        } else {
            match unary.op {
                UnaryOp::Neg => {
                    if helpers::is_numeric(&operand_ty) {
                        operand_ty
                    } else {
                        self.emit(TypeDiagnostic::UnaryOpMismatch {
                            op: unary.op.to_string(),
                            operand: operand_ty.to_string(),
                            span: self.span_for(unary.id),
                        });
                        Ty::Error
                    }
                }
                UnaryOp::Not => {
                    if operand_ty == Ty::Bool {
                        Ty::Bool
                    } else {
                        self.emit(TypeDiagnostic::UnaryOpMismatch {
                            op: unary.op.to_string(),
                            operand: operand_ty.to_string(),
                            span: self.span_for(unary.id),
                        });
                        Ty::Error
                    }
                }
            }
        };
        self.types.insert(unary.id, ty.clone());
        ty
    }

    fn infer_call(&mut self, call: &CallExpr) -> Ty {
        let callee_ty = self.resolve_callee(call);
        let arg_types: Vec<Ty> = call.args.iter().map(|a| self.infer_expr(a)).collect();

        let ty = if helpers::is_error(&callee_ty) {
            Ty::Error
        } else {
            match callee_ty {
                Ty::Fn(ref fn_ty) => self.check_call_args(call, fn_ty, &arg_types),
                _ => {
                    let name = helpers::call_name(&call.callee);
                    self.emit(TypeDiagnostic::NotCallable {
                        name,
                        found: callee_ty.to_string(),
                        span: self.span_for(call.id),
                    });
                    Ty::Error
                }
            }
        };
        self.types.insert(call.id, ty.clone());
        ty
    }

    fn resolve_callee(&mut self, call: &CallExpr) -> Ty {
        match &call.callee {
            NameRef::Resolved(r) => {
                if let Some(info) = self.env.lookup(&r.text) {
                    info.ty.clone()
                } else {
                    match helpers::builtin_fn(&r.text) {
                        Some(ty) => ty,
                        None => {
                            self.emit(TypeDiagnostic::UndefinedType {
                                name: r.text.clone(),
                                span: self.span_for(call.id),
                            });
                            Ty::Error
                        }
                    }
                }
            }
            NameRef::Unresolved(_) => Ty::Error,
        }
    }

    fn check_call_args(&mut self, call: &CallExpr, fn_ty: &FnTy, arg_types: &[Ty]) -> Ty {
        if fn_ty.params.len() != arg_types.len() {
            let name = helpers::call_name(&call.callee);
            self.emit(TypeDiagnostic::CallArityMismatch {
                name,
                expected: fn_ty.params.len(),
                found: arg_types.len(),
                span: self.span_for(call.id),
            });
            *fn_ty.return_type.clone()
        } else if Self::contains_type_param(&Ty::Fn(fn_ty.clone())) {
            // Generic call: unify arguments with parameter types, then
            // substitute type params in the return type.
            let mut subst = Substitution::new();
            for (arg_ty, param_ty) in arg_types.iter().zip(fn_ty.params.iter()) {
                if !helpers::is_error(arg_ty) && !helpers::is_error(param_ty) {
                    if let Err(found) = self.unify(arg_ty, param_ty, &mut subst) {
                        self.emit(TypeDiagnostic::TypeMismatch {
                            expected: param_ty.to_string(),
                            found: found.to_string(),
                            span: self.span_for(call.id),
                        });
                    }
                }
            }
            Self::substitute(&fn_ty.return_type, &subst)
        } else {
            // Non-generic call: structural equality check.
            for (i, (arg_ty, param_ty)) in arg_types.iter().zip(fn_ty.params.iter()).enumerate() {
                if !helpers::is_error(arg_ty) && !helpers::is_error(param_ty) && arg_ty != param_ty
                {
                    let _ = i;
                    self.emit(TypeDiagnostic::TypeMismatch {
                        expected: param_ty.to_string(),
                        found: arg_ty.to_string(),
                        span: self.span_for(call.id),
                    });
                }
            }
            *fn_ty.return_type.clone()
        }
    }

    fn infer_method_call(&mut self, mc: &MethodCallExpr) -> Ty {
        let _receiver_ty = self.infer_expr(&mc.receiver);
        for arg in &mc.args {
            self.infer_expr(arg);
        }
        self.emit(TypeDiagnostic::NotYetSupported {
            feature: "method calls".to_string(),
            span: self.span_for(mc.id),
        });
        self.types.insert(mc.id, Ty::Error);
        Ty::Error
    }

    fn infer_field(&mut self, field: &FieldExpr) -> Ty {
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

    fn infer_index(&mut self, index: &IndexExpr) -> Ty {
        self.infer_expr(&index.base);
        self.infer_expr(&index.index);
        self.emit(TypeDiagnostic::NotYetSupported {
            feature: "index expressions".to_string(),
            span: self.span_for(index.id),
        });
        self.types.insert(index.id, Ty::Error);
        Ty::Error
    }
}
