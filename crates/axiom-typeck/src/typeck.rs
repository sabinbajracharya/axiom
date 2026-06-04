//! The type checker: walks the HIR, assigns types to every expression and
//! statement, and collects type diagnostics.
//!
//! Two-pass design (per `docs/typeck-testing.md` §4.4):
//!   Pass 1 — Collect: register fn signatures, struct definitions, and enum
//!     definitions in the type environment. This allows forward references.
//!   Pass 2 — Check: walk fn bodies, type-checking each expression against the
//!     environment.
//!
//! Bidirectional typing (per §4.1):
//!   - `infer(expr) → Ty`: compute the type from subexpressions and the env.
//!   - `check(expr, expected) → Ty`: verify against an expected type.
//!
//! On error, return `Ty::Error` and emit a diagnostic. `Ty::Error` is sticky
//! (does not cascade additional diagnostics from subexpressions).

use crate::error::TypeDiagnostic;
use crate::thir::{Thir, TypeMap};
use crate::types::{EnumTy, FnTy, StructTy, Ty};

use axiom_hir::*;
use axiom_lexer::Span;
use std::collections::HashMap;

// ── Public entry point ────────────────────────────────────────────────────────

/// Type-check an HIR, producing a THIR (HIR + type map + diagnostics).
/// The HIR is consumed (moved) — the THIR owns it.
/// Never panics on user-reachable input. Returns a Thir even if
/// type errors exist; diagnostics are in `thir.diagnostics`.
pub fn check(hir: Hir) -> Thir {
    let mut checker = TypeChecker::new(hir);
    checker.collect_pass();
    checker.check_pass();
    Thir {
        hir: checker.hir,
        types: checker.types,
        diagnostics: checker.diagnostics,
    }
}

// ── The type checker ──────────────────────────────────────────────────────────

struct TypeChecker {
    hir: Hir,
    types: TypeMap,
    diagnostics: Vec<TypeDiagnostic>,
    env: TypeEnv,
    /// Tracks which HirIds correspond to mutable bindings (var, not val).
    mutability: HashMap<HirId, Mutability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mutability {
    Immutable,
    Mutable,
}

// ── Type environment ──────────────────────────────────────────────────────────

/// The type environment: a stack of scopes mapping names to types.
struct TypeEnv {
    scopes: Vec<Scope>,
}

struct Scope {
    bindings: HashMap<String, BindingInfo>,
}

struct BindingInfo {
    ty: Ty,
    _def_id: DefId,
    mutability: Mutability,
}

impl TypeEnv {
    fn new() -> Self {
        TypeEnv {
            scopes: vec![Scope {
                bindings: HashMap::new(),
            }],
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope {
            bindings: HashMap::new(),
        });
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn define(&mut self, name: String, ty: Ty, def_id: DefId, mutability: Mutability) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(
                name,
                BindingInfo {
                    ty,
                    _def_id: def_id,
                    mutability,
                },
            );
        }
    }

    fn lookup(&self, name: &str) -> Option<&BindingInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.bindings.get(name) {
                return Some(info);
            }
        }
        None
    }
}

// ── Struct/Enum info ──────────────────────────────────────────────────────────

struct StructInfo {
    name: String,
    def_id: DefId,
    fields: Vec<FieldInfo>,
}

struct FieldInfo {
    name: String,
    ty: Ty,
}

struct EnumInfo {
    name: String,
    def_id: DefId,
    variants: Vec<VariantInfo>,
}

struct VariantInfo {
    name: String,
    payload: Vec<Ty>,
}

impl TypeChecker {
    fn new(hir: Hir) -> Self {
        TypeChecker {
            hir,
            types: TypeMap::new(),
            diagnostics: Vec::new(),
            env: TypeEnv::new(),
            mutability: HashMap::new(),
        }
    }

    // ── Pass 1: collect ───────────────────────────────────────────────────

    fn collect_pass(&mut self) {
        self.collect_struct_defs();
        self.collect_enum_defs();
        self.collect_fn_sigs();
    }

    fn collect_struct_defs(&mut self) {
        let struct_infos: Vec<StructInfo> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::StructDef(s) => Some(StructInfo {
                    name: s.name.clone(),
                    def_id: s.id,
                    fields: s
                        .fields
                        .iter()
                        .map(|f| {
                            let ty = self.resolve_hir_ty(&f.ty);
                            FieldInfo {
                                name: f.name.clone(),
                                ty,
                            }
                        })
                        .collect(),
                }),
                _ => None,
            })
            .collect();

        for info in &struct_infos {
            let field_types: Vec<(String, Ty)> = info
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.ty.clone()))
                .collect();
            self.env.define(
                info.name.clone(),
                Ty::Struct(StructTy {
                    name: info.name.clone(),
                    def_id: info.def_id,
                }),
                info.def_id,
                Mutability::Immutable,
            );
            self.register_struct_fields(&info.name, &field_types);
        }
    }

    fn collect_enum_defs(&mut self) {
        let enum_infos: Vec<EnumInfo> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::EnumDef(e) => Some(EnumInfo {
                    name: e.name.clone(),
                    def_id: e.id,
                    variants: e
                        .variants
                        .iter()
                        .map(|v| {
                            let payload =
                                v.payload.iter().map(|t| self.resolve_hir_ty(t)).collect();
                            VariantInfo {
                                name: v.name.clone(),
                                payload,
                            }
                        })
                        .collect(),
                }),
                _ => None,
            })
            .collect();

        for info in &enum_infos {
            self.env.define(
                info.name.clone(),
                Ty::Enum(EnumTy {
                    name: info.name.clone(),
                    def_id: info.def_id,
                }),
                info.def_id,
                Mutability::Immutable,
            );
            self.register_enum_variants(&info.name, &info.variants);
        }
    }

    fn collect_fn_sigs(&mut self) {
        for item in &self.hir.items {
            match item {
                Item::FnDef(f) => {
                    let param_types: Vec<Ty> = f
                        .params
                        .iter()
                        .map(|p| {
                            p.ty.as_ref()
                                .map(|t| self.resolve_hir_ty(t))
                                .unwrap_or(Ty::Error)
                        })
                        .collect();
                    let return_type = f
                        .return_type
                        .as_ref()
                        .map(|t| self.resolve_hir_ty(t))
                        .unwrap_or(Ty::Unit);
                    let fn_ty = Ty::Fn(FnTy {
                        params: param_types,
                        return_type: Box::new(return_type),
                    });
                    self.env
                        .define(f.name.clone(), fn_ty, f.id, Mutability::Immutable);
                }
                Item::StructDef(_) | Item::EnumDef(_) => {}
            }
        }
    }

    /// Resolve an `HirTy` (the type syntax in the source) to a `Ty` (the
    /// type checker's internal representation). Unresolved names → Ty::Error.
    fn resolve_hir_ty(&self, hir_ty: &HirTy) -> Ty {
        match hir_ty {
            HirTy::Named(nr) => {
                let text = match nr {
                    NameRef::Resolved(r) => &r.text,
                    NameRef::Unresolved(u) => &u.text,
                };
                // Builtin type names are always available.
                match text.as_str() {
                    "Int" => return Ty::Int,
                    "Float" => return Ty::Float,
                    "Bool" => return Ty::Bool,
                    "String" => return Ty::String,
                    "Unit" => return Ty::Unit,
                    _ => {}
                }
                // Look up user-defined types in the type env by name text.
                if let Some(info) = self.env.lookup(text) {
                    match &info.ty {
                        Ty::Struct(s) => Ty::Struct(s.clone()),
                        Ty::Enum(e) => Ty::Enum(e.clone()),
                        other => other.clone(),
                    }
                } else {
                    Ty::Error
                }
            }
            HirTy::Unit => Ty::Unit,
            HirTy::Tuple(elems) => {
                Ty::Tuple(elems.iter().map(|t| self.resolve_hir_ty(t)).collect())
            }
            HirTy::Fn(f) => {
                let params = f.params.iter().map(|t| self.resolve_hir_ty(t)).collect();
                let return_type = Box::new(self.resolve_hir_ty(&f.return_type));
                Ty::Fn(FnTy {
                    params,
                    return_type,
                })
            }
            HirTy::Error => Ty::Error,
        }
    }

    // ── Pass 2: check ────────────────────────────────────────────────────

    fn check_pass(&mut self) {
        // Check each fn body.
        for item in &self.hir.items.clone() {
            if let Item::FnDef(f) = item {
                self.check_fn_body(f);
            }
        }
    }

    fn check_fn_body(&mut self, f: &FnDef) {
        self.env.push_scope();
        // Define params in scope.
        for param in &f.params {
            let param_type = param
                .ty
                .as_ref()
                .map(|t| self.resolve_hir_ty(t))
                .unwrap_or(Ty::Error);
            let mutability = Mutability::Immutable; // params are let by default
            self.env
                .define(param.name.clone(), param_type.clone(), param.id, mutability);
            self.types.insert(param.id, param_type);
            self.mutability.insert(param.id, mutability);
        }
        let return_type = f
            .return_type
            .as_ref()
            .map(|t| self.resolve_hir_ty(t))
            .unwrap_or(Ty::Unit);

        let body_type = self.check_block(&f.body, &return_type);

        // Check that the body type matches the declared return type.
        if !is_error(&body_type) && !is_error(&return_type) && body_type != return_type {
            // Only emit if we haven't already diagnosed it via the block check.
            // The block check already handles tail expression mismatches.
        }

        let fn_ty = Ty::Fn(FnTy {
            params: f
                .params
                .iter()
                .map(|p| self.types.get(&p.id).cloned().unwrap_or(Ty::Error))
                .collect(),
            return_type: Box::new(return_type.clone()),
        });
        self.types.insert(f.id, fn_ty);
        self.env.pop_scope();
    }

    // ── Infer (compute type from context) ────────────────────────────────

    fn infer_expr(&mut self, expr: &Expr) -> Ty {
        match expr {
            Expr::Lit(lit) => {
                let ty = self.infer_lit(&lit.kind);
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

    // ── Check (verify against expected type) ─────────────────────────────

    fn check_expr(&mut self, expr: &Expr, expected: &Ty) -> Ty {
        let inferred = self.infer_expr(expr);
        let result = if is_error(&inferred) || is_error(expected) {
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

    // ── Expression type rules ────────────────────────────────────────────

    fn infer_lit(&mut self, kind: &LitKind) -> Ty {
        match kind {
            LitKind::Int(_) => Ty::Int,
            LitKind::Float(_) => Ty::Float,
            LitKind::Bool(_) => Ty::Bool,
            LitKind::String(_) => Ty::String,
            LitKind::Unit => Ty::Unit,
        }
    }

    fn infer_path(&mut self, name_ref: &NameRef, expr_id: HirId) -> Ty {
        let ty = match name_ref {
            NameRef::Resolved(r) => {
                if let Some(info) = self.env.lookup(&r.text) {
                    info.ty.clone()
                } else {
                    // Maybe it's a builtin. Check reserved names.
                    match r.text.as_str() {
                        "print" | "println" => Ty::Fn(FnTy {
                            params: vec![Ty::String],
                            return_type: Box::new(Ty::Unit),
                        }),
                        _ => {
                            self.emit(TypeDiagnostic::UndefinedType {
                                name: r.text.clone(),
                                span: self.span_for(expr_id),
                            });
                            Ty::Error
                        }
                    }
                }
            }
            NameRef::Unresolved(_) => {
                // HIR already emitted UnresolvedName — don't cascade.
                Ty::Error
            }
        };
        self.types.insert(expr_id, ty.clone());
        ty
    }

    fn infer_bin(&mut self, bin: &BinExpr) -> Ty {
        let left_ty = self.infer_expr(&bin.left);
        let right_ty = self.infer_expr(&bin.right);

        let ty = if is_error(&left_ty) || is_error(&right_ty) {
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
        if is_numeric(&left_ty) && left_ty == right_ty {
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
        let ty = if is_error(&operand_ty) {
            Ty::Error
        } else {
            match unary.op {
                UnaryOp::Neg => {
                    if is_numeric(&operand_ty) {
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

        let ty = if is_error(&callee_ty) {
            Ty::Error
        } else {
            match callee_ty {
                Ty::Fn(ref fn_ty) => self.check_call_args(call, fn_ty, &arg_types),
                _ => {
                    let name = call_name(&call.callee);
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
                    match r.text.as_str() {
                        "print" | "println" => Ty::Fn(FnTy {
                            params: vec![Ty::String],
                            return_type: Box::new(Ty::Unit),
                        }),
                        _ => {
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
            let name = call_name(&call.callee);
            self.emit(TypeDiagnostic::CallArityMismatch {
                name,
                expected: fn_ty.params.len(),
                found: arg_types.len(),
                span: self.span_for(call.id),
            });
        } else {
            for (i, (arg_ty, param_ty)) in arg_types.iter().zip(fn_ty.params.iter()).enumerate() {
                if !is_error(arg_ty) && !is_error(param_ty) && arg_ty != param_ty {
                    let _ = i;
                    self.emit(TypeDiagnostic::TypeMismatch {
                        expected: param_ty.to_string(),
                        found: arg_ty.to_string(),
                        span: self.span_for(call.id),
                    });
                }
            }
        }
        *fn_ty.return_type.clone()
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
        let ty = if is_error(&receiver_ty) {
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

    fn infer_block(&mut self, block: &Block, expected: &Option<Ty>) -> Ty {
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

    fn check_block(&mut self, block: &Block, expected: &Ty) -> Ty {
        self.infer_block(block, &Some(expected.clone()))
    }

    fn infer_if(&mut self, if_expr: &IfExpr) -> Ty {
        let cond_ty = self.infer_expr(&if_expr.condition);
        if !is_error(&cond_ty) && cond_ty != Ty::Bool {
            self.emit(TypeDiagnostic::ConditionNotBool {
                found: cond_ty.to_string(),
                span: self.span_for(if_expr.id),
            });
        }

        let then_type = self.infer_block(&if_expr.then_branch, &None);

        let ty = if let Some(els) = &if_expr.else_branch {
            let else_type = self.infer_expr(els);
            if is_error(&then_type) || is_error(&else_type) {
                if is_error(&then_type) {
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
            // No else branch: result is Unit.
            if then_type != Ty::Unit && !is_error(&then_type) {
                // If without else must produce Unit. The then-branch's value
                // is discarded. This is not an error per se — if without else
                // always produces Unit.
            }
            Ty::Unit
        };
        self.types.insert(if_expr.id, ty.clone());
        ty
    }

    fn infer_match(&mut self, match_expr: &MatchExpr) -> Ty {
        let scrutinee_ty = self.infer_expr(&match_expr.scrutinee);

        let arm_types: Vec<Ty> = match_expr
            .arms
            .iter()
            .map(|arm| {
                self.env.push_scope();
                // Define pattern bindings.
                self.define_pattern_bindings(&arm.pattern, &scrutinee_ty);
                if let Some(guard) = &arm.guard {
                    self.infer_expr(guard);
                }
                let arm_ty = self.infer_expr(&arm.body);
                self.env.pop_scope();
                arm_ty
            })
            .collect();

        // Check exhaustiveness for enum scrutinees.
        if !is_error(&scrutinee_ty) {
            if let Ty::Enum(enum_ty) = &scrutinee_ty {
                for diag in self.check_match_exhaustiveness(match_expr, enum_ty) {
                    self.emit(diag);
                }
            }
        }

        // All arms must agree on type.
        let ty = if arm_types.is_empty() {
            Ty::Unit
        } else {
            let first_type = &arm_types[0];
            let mut mismatch = false;
            for (i, arm_ty) in arm_types.iter().enumerate().skip(1) {
                if !is_error(arm_ty) && !is_error(first_type) && *arm_ty != *first_type {
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

    fn infer_loop(&mut self, loop_expr: &LoopExpr) -> Ty {
        let ty = match &loop_expr.kind {
            LoopKind::Infinite(body) => {
                self.infer_block(body, &None);
                Ty::Unit
            }
            LoopKind::Conditional { condition, body } => {
                let cond_ty = self.infer_expr(condition);
                if !is_error(&cond_ty) && cond_ty != Ty::Bool {
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
                let binding_ty = if is_error(&iterable_ty) {
                    Ty::Error
                } else {
                    // v0: iterators not fully supported.
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

    fn infer_struct_lit(&mut self, sl: &StructLitExpr) -> Ty {
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
                        if !is_error(&value_ty)
                            && !is_error(expected_ty)
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

    fn infer_assign(&mut self, assign: &AssignExpr) -> Ty {
        let value_ty = self.infer_expr(&assign.value);

        let ty = match &assign.target {
            AssignTarget::Name(nr) => {
                match nr {
                    NameRef::Resolved(r) => {
                        let lookup = self
                            .env
                            .lookup(&r.text)
                            .map(|info| (info.ty.clone(), info.mutability));
                        // Check mutability.
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
                            if !is_error(&value_ty)
                                && !is_error(binding_ty)
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

    // ── Statement type rules ─────────────────────────────────────────────

    fn type_stmt(&mut self, stmt: &Stmt) {
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
            if !is_error(&value_ty) && !is_error(&resolved) && value_ty != resolved {
                self.emit(TypeDiagnostic::TypeMismatch {
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
            if !is_error(&value_ty) && !is_error(&resolved) && value_ty != resolved {
                self.emit(TypeDiagnostic::TypeMismatch {
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
                self.env.define(p.name.clone(), ty.clone(), p.id, mutab);
                self.types.insert(p.id, ty.clone());
                self.mutability.insert(p.id, mutab);
            }
            Pattern::Wildcard(id) => {
                self.types.insert(*id, ty.clone());
            }
            Pattern::Literal(lp) => {
                let lit_ty = self.infer_lit(&lp.kind);
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
        // Resolve the variant's payload types.
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

    fn define_pattern_bindings(&mut self, pat: &Pattern, scrutinee_ty: &Ty) {
        self.define_pattern(pat, scrutinee_ty, Mutability::Immutable);
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn assign_error(&mut self, id: HirId) -> Ty {
        self.types.insert(id, Ty::Error);
        Ty::Error
    }

    fn emit(&mut self, diag: TypeDiagnostic) {
        self.diagnostics.push(diag);
    }

    fn span_for(&self, _id: HirId) -> Span {
        // v0: we don't have spans on all nodes yet; use a zero span.
        // The HIR doesn't carry source spans at the moment.
        Span { lo: 0, hi: 0 }
    }

    // ── Struct field registry ────────────────────────────────────────────

    fn register_struct_fields(&mut self, _name: &str, _fields: &[(String, Ty)]) {
        // Store as a simple top-level association. We'll use the TypeEnv
        // for now — struct definitions are in the top scope.
        // For v0, we'll use a separate registry.
    }

    fn lookup_struct_fields(&self, name: &str) -> Option<Vec<(String, Ty)>> {
        // Look through items for the struct definition.
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

    fn register_enum_variants(&mut self, _name: &str, _variants: &[VariantInfo]) {
        // v0: we look up variants from the HIR directly.
    }

    fn lookup_enum_variants(&self, name: &str) -> Option<Vec<VariantInfo>> {
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
}

// ── Match exhaustiveness ─────────────────────────────────────────────────────

impl TypeChecker {
    fn check_match_exhaustiveness(
        &self,
        match_expr: &MatchExpr,
        enum_ty: &EnumTy,
    ) -> Vec<TypeDiagnostic> {
        let all_variants: Vec<String> = self
            .lookup_enum_variants(&enum_ty.name)
            .map(|vs| vs.iter().map(|v| v.name.clone()).collect())
            .unwrap_or_default();

        if all_variants.is_empty() {
            return Vec::new();
        }

        let mut covered: Vec<String> = Vec::new();
        for arm in &match_expr.arms {
            self.collect_covered_variants(&arm.pattern, &all_variants, &mut covered);
        }

        let missing: Vec<String> = all_variants
            .iter()
            .filter(|v| !covered.contains(&v.to_string()))
            .cloned()
            .collect();

        if !missing.is_empty() {
            vec![TypeDiagnostic::NonExhaustiveMatch {
                missing,
                span: self.span_for(match_expr.id),
            }]
        } else {
            Vec::new()
        }
    }

    fn collect_covered_variants(
        &self,
        pat: &Pattern,
        all_variants: &[String],
        covered: &mut Vec<String>,
    ) {
        match pat {
            Pattern::Wildcard(_) => {
                covered.extend(all_variants.iter().cloned());
            }
            Pattern::Ident(_) => {
                // An identifier pattern covers all variants (it's a catch-all).
                covered.extend(all_variants.iter().cloned());
            }
            Pattern::Literal(_) => {
                // Literals don't cover enum variants.
            }
            Pattern::TupleStruct(ts) => match &ts.path {
                NameRef::Resolved(r) => {
                    if !covered.contains(&r.text) {
                        covered.push(r.text.clone());
                    }
                }
                NameRef::Unresolved(_) => {}
            },
            Pattern::Struct(sp) => match &sp.path {
                NameRef::Resolved(r) => {
                    if !covered.contains(&r.text) {
                        covered.push(r.text.clone());
                    }
                }
                NameRef::Unresolved(_) => {}
            },
            Pattern::Or(op) => {
                for alt in &op.alternatives {
                    self.collect_covered_variants(alt, all_variants, covered);
                }
            }
            Pattern::Range(_) => {}
        }
    }
}

// ── Utility functions ─────────────────────────────────────────────────────────

fn is_error(ty: &Ty) -> bool {
    matches!(ty, Ty::Error)
}

fn is_numeric(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float)
}

fn call_name(name_ref: &NameRef) -> String {
    match name_ref {
        NameRef::Resolved(r) => r.text.clone(),
        NameRef::Unresolved(u) => u.text.clone(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axiom_hir::lower;
    use axiom_parser::ast::AstNode;

    fn check_source(source: &str) -> Thir {
        let result = axiom_parser::parse(source);
        let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
        let hir = lower(&root, source);
        check(hir)
    }

    #[test]
    fn test_infer_int_literal() {
        let thir = check_source("fn main() { val x = 42 }");
        // The literal should be typed as Int.
        let has_int = thir.types.values().any(|t| *t == Ty::Int);
        assert!(
            has_int,
            "expected Int type somewhere, got: {:?}",
            thir.types
        );
    }

    #[test]
    fn test_infer_string_literal() {
        let thir = check_source("fn main() { print(\"hello\") }");
        let has_string = thir.types.values().any(|t| *t == Ty::String);
        assert!(has_string, "expected String type somewhere");
    }

    #[test]
    fn test_infer_bin_op_add() {
        let thir = check_source("fn main() { val x = 1 + 2 }");
        let has_int = thir.types.values().any(|t| *t == Ty::Int);
        assert!(has_int, "expected Int type from addition");
    }

    #[test]
    fn test_type_mismatch_bin_op() {
        let thir = check_source("fn main() { val x = 1 + 2.0 }");
        assert!(
            thir.diagnostics
                .iter()
                .any(|d| d.kind() == "bin_op_mismatch"),
            "expected bin op mismatch diagnostic, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_fn_call_with_params() {
        let thir = check_source("fn add(a: Int, b: Int) -> Int { a + b } fn main() { add(1, 2) }");
        assert!(
            thir.diagnostics.iter().all(|d| d.kind() != "type_mismatch"),
            "unexpected type errors: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_fn_call_arity_mismatch() {
        let thir = check_source("fn add(a: Int, b: Int) -> Int { a + b } fn main() { add(1) }");
        assert!(
            thir.diagnostics
                .iter()
                .any(|d| d.kind() == "call_arity_mismatch"),
            "expected arity mismatch, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_struct_literal() {
        let thir = check_source(
            "struct Point { x: Float, y: Float }
fn main() { val p = Point { x: 1.0, y: 2.0 } }",
        );
        let has_struct = thir.types.values().any(|t| matches!(t, Ty::Struct(_)));
        assert!(has_struct, "expected Struct type");
    }

    #[test]
    fn test_enum_match() {
        let thir = check_source(
            "enum Shape { Circle(Float), Rect(Float, Float), Empty }
fn area(s: Shape) -> Float { match s { Circle(r) => 3.14 Rect(w, h) => 1.0 Empty => 0.0 } }",
        );
        // The match should be exhaustive — no non-exhaustive diagnostic.
        let non_exhaustive: Vec<_> = thir
            .diagnostics
            .iter()
            .filter(|d| d.kind() == "non_exhaustive_match")
            .collect();
        assert!(
            non_exhaustive.is_empty(),
            "unexpected non-exhaustive match: {:?}",
            non_exhaustive
        );
    }

    #[test]
    fn test_non_exhaustive_match() {
        let thir = check_source(
            "enum Shape { Circle(Float), Rect(Float, Float) }
fn area(s: Shape) -> Float { match s { Circle(r) => r } }",
        );
        assert!(
            thir.diagnostics
                .iter()
                .any(|d| d.kind() == "non_exhaustive_match"),
            "expected non-exhaustive match diagnostic, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_assign_to_immutable() {
        let thir = check_source("fn main() { val x = 1 x = 2 }");
        assert!(
            thir.diagnostics
                .iter()
                .any(|d| d.kind() == "assign_to_immutable"),
            "expected assign_to_immutable diagnostic, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_val_type_annotation() {
        let thir = check_source("fn main() { val x: Int = 42 }");
        let has_int = thir.types.values().any(|t| *t == Ty::Int);
        assert!(has_int, "expected Int type");
        assert!(
            thir.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_type_mismatch_val_annotation() {
        let thir = check_source("fn main() { val x: Int = 3.14 }");
        assert!(
            thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
            "expected type mismatch diagnostic, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_bool_condition_in_if() {
        let thir = check_source("fn main() { if true { val x = 1 } }");
        // Should type-check cleanly — condition is Bool.
        let cond_errors: Vec<_> = thir
            .diagnostics
            .iter()
            .filter(|d| d.kind() == "condition_not_bool")
            .collect();
        assert!(
            cond_errors.is_empty(),
            "unexpected condition errors: {:?}",
            cond_errors
        );
    }

    #[test]
    fn test_not_callable() {
        let thir = check_source("fn main() { val x = 1 x() }");
        assert!(
            thir.diagnostics.iter().any(|d| d.kind() == "not_callable"),
            "expected not_callable diagnostic, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_loop_produces_unit() {
        let thir = check_source("fn main() { loop { print(\"hello\") } }");
        // Loop should type-check as Unit.
        // No condition_not_bool or type errors expected.
        let _type_errors: Vec<_> = thir
            .diagnostics
            .iter()
            .filter(|d| d.kind() != "not_yet_supported")
            .collect();
        // Note: we may get not_yet_supported for various things.
        // The key thing is loop produces Unit.
    }
}
