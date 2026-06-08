//! Block, statement, and expression name resolution.

use super::{resolve_name_ref, Scope};
use crate::hir_types::*;
use crate::lowering::DefKind;
use crate::HirDiagnostic;
use std::collections::HashMap;

pub(super) fn resolve_block_names(
    block: &mut Block,
    parent_scope: &Scope,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    let mut scope = Scope::new_child(&parent_scope.bindings);
    for stmt in &mut block.stmts {
        resolve_stmt_names(stmt, &mut scope, diagnostics);
    }
    if let Some(tail) = &mut block.tail {
        resolve_expr_names(tail, &mut scope, diagnostics);
    }
}

fn resolve_stmt_names(stmt: &mut Stmt, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    match stmt {
        Stmt::ValStmt(s) => {
            resolve_expr_names(&mut s.value, scope, diagnostics);
            define_pattern_bindings(&mut s.pattern, scope, diagnostics);
        }
        Stmt::VarStmt(s) => {
            resolve_expr_names(&mut s.value, scope, diagnostics);
            define_pattern_bindings(&mut s.pattern, scope, diagnostics);
        }
        Stmt::ExprStmt(s) => {
            resolve_expr_names(&mut s.expr, scope, diagnostics);
        }
        Stmt::ReturnStmt(s) => {
            if let Some(v) = &mut s.value {
                resolve_expr_names(v, scope, diagnostics);
            }
        }
        Stmt::BreakStmt(s) => {
            if let Some(v) = &mut s.value {
                resolve_expr_names(v, scope, diagnostics);
            }
        }
        Stmt::ContinueStmt(_) => {}
        Stmt::YieldStmt(s) => {
            resolve_expr_names(&mut s.value, scope, diagnostics);
        }
    }
}

pub(super) fn define_pattern_bindings(
    pat: &mut Pattern,
    scope: &mut Scope,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    match pat {
        Pattern::Ident(p) => {
            if scope.define(p.name.clone(), p.id, DefKind::Local) {
                diagnostics.push(HirDiagnostic::DuplicateDefinition {
                    name: p.name.clone(),
                    span: p.span,
                });
            }
            p.binding = Some(p.id);
        }
        Pattern::Wildcard(_) | Pattern::Literal(_) | Pattern::Range(_) => {}
        Pattern::TupleStruct(ts) => {
            resolve_name_ref(&mut ts.path, &scope.bindings, diagnostics);
            for field in &mut ts.fields {
                define_pattern_bindings(field, scope, diagnostics);
            }
        }
        Pattern::Struct(sp) => {
            resolve_name_ref(&mut sp.path, &scope.bindings, diagnostics);
            for field in &mut sp.fields {
                define_pattern_bindings(&mut field.pattern, scope, diagnostics);
            }
        }
        Pattern::Or(op) => {
            for alt in &mut op.alternatives {
                define_pattern_bindings(alt, scope, diagnostics);
            }
        }
    }
}

fn resolve_expr_names(expr: &mut Expr, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    match expr {
        Expr::Lit(_) => {}
        Expr::Path(p) => {
            resolve_name_ref(&mut p.name_ref, &scope.bindings, diagnostics);
        }
        Expr::Bin(b) => {
            resolve_expr_names(&mut b.left, scope, diagnostics);
            resolve_expr_names(&mut b.right, scope, diagnostics);
        }
        Expr::Unary(u) => {
            resolve_expr_names(&mut u.operand, scope, diagnostics);
        }
        Expr::Call(c) => resolve_call_names(c, scope, diagnostics),
        Expr::MethodCall(m) => resolve_method_call_names(m, scope, diagnostics),
        Expr::Field(f) => {
            resolve_expr_names(&mut f.receiver, scope, diagnostics);
        }
        Expr::Index(i) => {
            resolve_expr_names(&mut i.base, scope, diagnostics);
            for index in &mut i.indices {
                resolve_expr_names(index, scope, diagnostics);
            }
        }
        Expr::Block(b) => {
            resolve_block_names(b, scope, diagnostics);
        }
        Expr::If(i) => resolve_if_names(i, scope, diagnostics),
        Expr::Match(m) => resolve_match_names(m, scope, diagnostics),
        Expr::Loop(l) => resolve_loop_names(l, scope, diagnostics),
        Expr::StructLit(s) => resolve_struct_lit_names(s, scope, diagnostics),
        Expr::Assign(a) => {
            resolve_assign_target_names(&mut a.target, scope, diagnostics);
            resolve_expr_names(&mut a.value, scope, diagnostics);
        }
        Expr::ListLit(l) => {
            for elem in &mut l.elements {
                resolve_expr_names(elem, scope, diagnostics);
            }
        }
    }
}

fn resolve_call_names(c: &mut CallExpr, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    if c.qualifier.is_some() {
        // Qualified call (`Type::method()`): the callee segment is an associated
        // function or method, resolved by the type checker against the
        // qualifier's type — not as a bare name here. Attempt a bare-name
        // resolution anyway so enum constructors (`Maybe::Just`) still bind, but
        // don't error if it isn't a top-level name (the type checker will, if it
        // is genuinely missing).
        super::try_resolve_name_ref(&mut c.callee, &scope.bindings);
    } else {
        resolve_name_ref(&mut c.callee, &scope.bindings, diagnostics);
    }
    for arg in &mut c.args {
        resolve_expr_names(arg, scope, diagnostics);
    }
}

fn resolve_method_call_names(
    m: &mut MethodCallExpr,
    scope: &mut Scope,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    resolve_expr_names(&mut m.receiver, scope, diagnostics);
    for arg in &mut m.args {
        resolve_expr_names(arg, scope, diagnostics);
    }
}

fn resolve_if_names(i: &mut IfExpr, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    resolve_expr_names(&mut i.condition, scope, diagnostics);
    resolve_block_names(&mut i.then_branch, scope, diagnostics);
    if let Some(els) = &mut i.else_branch {
        resolve_expr_names(els, scope, diagnostics);
    }
}

fn resolve_match_names(m: &mut MatchExpr, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    resolve_expr_names(&mut m.scrutinee, scope, diagnostics);
    for arm in &mut m.arms {
        let mut arm_scope = Scope::new_child(&scope.bindings);
        resolve_pattern_names(&mut arm.pattern, &arm_scope.bindings, diagnostics);
        define_pattern_bindings(&mut arm.pattern, &mut arm_scope, diagnostics);
        if let Some(g) = &mut arm.guard {
            resolve_expr_names(g, &mut arm_scope, diagnostics);
        }
        resolve_expr_names(&mut arm.body, &mut arm_scope, diagnostics);
    }
}

fn resolve_loop_names(l: &mut LoopExpr, scope: &mut Scope, diagnostics: &mut Vec<HirDiagnostic>) {
    match &mut l.kind {
        LoopKind::Infinite(body) => {
            resolve_block_names(body, scope, diagnostics);
        }
        LoopKind::Conditional { condition, body } => {
            resolve_expr_names(condition, scope, diagnostics);
            resolve_block_names(body, scope, diagnostics);
        }
        LoopKind::Iterator {
            binding,
            binding_id,
            iterable,
            body,
        } => {
            resolve_expr_names(iterable, scope, diagnostics);
            scope.define(binding.clone(), *binding_id, DefKind::Local);
            resolve_block_names(body, scope, diagnostics);
        }
    }
}

fn resolve_struct_lit_names(
    s: &mut StructLitExpr,
    scope: &mut Scope,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    resolve_name_ref(&mut s.type_name, &scope.bindings, diagnostics);
    for field in &mut s.fields {
        resolve_expr_names(&mut field.value, scope, diagnostics);
    }
}

fn resolve_assign_target_names(
    target: &mut AssignTarget,
    scope: &mut Scope,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    match target {
        AssignTarget::Name(nr) => {
            resolve_name_ref(nr, &scope.bindings, diagnostics);
        }
        AssignTarget::Field { receiver, field: _ } => {
            resolve_expr_names(receiver, scope, diagnostics);
        }
        AssignTarget::Index { base, indices } => {
            resolve_expr_names(base, scope, diagnostics);
            for index in indices {
                resolve_expr_names(index, scope, diagnostics);
            }
        }
    }
}

pub(super) fn resolve_pattern_names(
    pat: &mut Pattern,
    bindings: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    match pat {
        Pattern::Wildcard(_) | Pattern::Literal(_) | Pattern::Range(_) => {}
        Pattern::Ident(_p) => {
            // Ident patterns introduce bindings (handled in define_pattern_bindings).
        }
        Pattern::TupleStruct(ts) => {
            resolve_name_ref(&mut ts.path, bindings, diagnostics);
            for field in &mut ts.fields {
                resolve_pattern_names(field, bindings, diagnostics);
            }
        }
        Pattern::Struct(sp) => {
            resolve_name_ref(&mut sp.path, bindings, diagnostics);
            for field in &mut sp.fields {
                resolve_pattern_names(&mut field.pattern, bindings, diagnostics);
            }
        }
        Pattern::Or(op) => {
            for alt in &mut op.alternatives {
                resolve_pattern_names(alt, bindings, diagnostics);
            }
        }
    }
}
