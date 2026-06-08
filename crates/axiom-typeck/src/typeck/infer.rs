//! Leaf expression type inference: literals, paths, binary/unary ops, calls.

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
            Expr::ListLit(_) => {
                // ListLit should be desugared before typeck, but the bare
                // `check()` path (no-stdlib) cannot desugar (missing lang
                // items). Fall back to an error until the old infer_list_lit
                // special-case is fully replaced.
                self.emit(TypeDiagnostic::NotYetSupported {
                    feature: "list literals without stdlib".to_string(),
                    span: axiom_lexer::Span { lo: 0, hi: 0 },
                });
                Ty::Error
            },
        }
    }

    pub(super) fn check_expr(&mut self, expr: &Expr, expected: &Ty) -> Ty {
        let inferred = self.infer_expr(expr);
        let result = if helpers::is_error(&inferred) || helpers::is_error(expected) {
            inferred.clone()
        } else if inferred == *expected || self.unifies_either_way(&inferred, expected) {
            // Adopt the expected type when it matches, or when the two unify
            // modulo type parameters — e.g. a generic struct literal whose
            // phantom parameters aren't pinned by its fields (`List::new`'s
            // `List { buf: heap_alloc(0), … }`) takes the declared `List<T>`.
            expected.clone()
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

    /// Do `a` and `b` unify when type parameters on *either* side may bind? Used
    /// by [`check_expr`] to accept a value whose type carries still-open type
    /// parameters against an expected type (and vice versa). Each direction uses
    /// a fresh substitution — this is a compatibility test, not a binding step.
    fn unifies_either_way(&self, a: &Ty, b: &Ty) -> bool {
        self.unify(a, b, &mut Substitution::new()).is_ok()
            || self.unify(b, a, &mut Substitution::new()).is_ok()
    }

    fn infer_path(&mut self, name_ref: &NameRef, expr_id: HirId) -> Ty {
        let ty = match name_ref {
            NameRef::Resolved(r) => {
                if r.text == "Self" {
                    if let Some(ref self_ty) = self.current_self_type {
                        self_ty.clone()
                    } else {
                        Ty::Error
                    }
                } else if let Some(info) = self.env.lookup(&r.text) {
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
            NameRef::Unresolved(u) => {
                if u.text == "Self" {
                    if let Some(ref self_ty) = self.current_self_type {
                        self_ty.clone()
                    } else {
                        Ty::Error
                    }
                } else {
                    Ty::Error
                }
            }
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
        // `format` is the one variadic intrinsic (the formatting primitive, §11).
        // It accepts any number of arguments of any type and yields `String`;
        // the runtime template engine decides how each is rendered. A user fn
        // named `format` shadows it (handled by the `env.lookup` guard).
        if self.is_format_intrinsic(&call.callee) {
            for arg in &call.args {
                self.infer_expr(arg);
            }
            let ty = Ty::String;
            self.types.insert(call.id, ty.clone());
            return ty;
        }

        let arg_types: Vec<Ty> = call.args.iter().map(|a| self.infer_expr(a)).collect();

        // A qualified call may name an associated function (`List::new()`) — a
        // method with no `self`. Enum constructors and module-qualified calls
        // fall through to ordinary callee resolution below.
        if let Some(ty) = self.try_assoc_fn_call(call, &arg_types) {
            self.types.insert(call.id, ty.clone());
            return ty;
        }

        let callee_ty = self.resolve_callee(call);

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

    /// Is this call the `format` intrinsic? True when the callee names `format`
    /// and no user/stdlib function of that name is in scope to shadow it.
    fn is_format_intrinsic(&self, callee: &NameRef) -> bool {
        let name = match callee {
            NameRef::Resolved(r) => &r.text,
            NameRef::Unresolved(u) => &u.text,
        };
        name == "format" && self.env.lookup(name).is_none()
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
            // An unresolved callee has no FnDef — but it may still name a
            // compiler intrinsic (e.g. the `heap_*` floor ops, which have no
            // library definition). Consult `builtin_fn` before giving up.
            NameRef::Unresolved(u) => helpers::builtin_fn(&u.text).unwrap_or(Ty::Error),
        }
    }

    pub(super) fn check_call_args(
        &mut self,
        call: &CallExpr,
        fn_ty: &FnTy,
        arg_types: &[Ty],
    ) -> Ty {
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
            self.check_type_bounds(&subst, self.span_for(call.id));
            Self::substitute(&fn_ty.return_type, &subst)
        } else {
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

    /// Check that each concrete type in `subst` satisfies the trait bounds
    /// declared on its type parameter.
    fn check_type_bounds(
        &mut self,
        subst: &std::collections::HashMap<crate::types::TypeParamId, Ty>,
        span: axiom_lexer::Span,
    ) {
        for (tp_id, concrete_ty) in subst {
            let bounds: Vec<String> = self
                .type_param_bounds
                .get(&tp_id.def_id)
                .cloned()
                .unwrap_or_default();

            let type_name = match Self::type_name_from_ty(concrete_ty) {
                Some(n) => n,
                None => continue,
            };

            for bound in &bounds {
                self.check_single_bound(bound, &type_name, tp_id, span);
            }
        }
    }

    /// Check that a single trait bound (and its supertraits) are satisfied.
    fn check_single_bound(
        &mut self,
        bound: &str,
        type_name: &str,
        tp_id: &crate::types::TypeParamId,
        span: axiom_lexer::Span,
    ) {
        let has_impl = self
            .impl_table
            .iter()
            .any(|info| info.trait_name.as_deref() == Some(bound) && info.type_name == type_name);
        if !has_impl {
            self.emit(TypeDiagnostic::UnsatisfiedBound {
                type_name: type_name.to_string(),
                bound: bound.to_string(),
                param: tp_id.name.clone(),
                span,
            });
            return;
        }
        if let Some(trait_info) = self.trait_registry.get(bound).cloned() {
            for supertrait in &trait_info.supertraits {
                self.check_single_bound(supertrait, type_name, tp_id, span);
            }
        }
    }

    /// Extract the type name string used in the impl table from a `Ty`.
    pub(super) fn type_name_from_ty(ty: &Ty) -> Option<String> {
        match ty {
            Ty::Struct(s) => Some(s.name.clone()),
            Ty::Enum(e) => Some(e.name.clone()),
            Ty::Instance(inst) => Some(inst.name.clone()),
            Ty::Int => Some("Int".to_string()),
            Ty::Float => Some("Float".to_string()),
            Ty::Bool => Some("Bool".to_string()),
            Ty::String => Some("String".to_string()),
            _ => None,
        }
    }
}
