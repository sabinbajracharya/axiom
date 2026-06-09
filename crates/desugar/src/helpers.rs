//! Shared utilities for desugar passes: walk functions, ID generation, temp names.

use resolver::{hir_types::*, lang::LangItems};

/// Context for generating fresh IDs and temp names during desugaring.
pub(crate) struct DesugarCtx<'a> {
    pub lang_items: &'a LangItems,
    pub next_id: usize,
    pub temp_counter: usize,
    /// Names resolved by the desugar pass (e.g. `catch |e|` capture variables).
    /// Used to clean up stale `UnresolvedName` diagnostics after desugaring.
    pub fixed_names: Vec<String>,
}

impl DesugarCtx<'_> {
    pub fn fresh_id(&mut self) -> HirId {
        let id = HirId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn fresh_temp_name(&mut self) -> String {
        let name = format!("__list_{}", self.temp_counter);
        self.temp_counter += 1;
        name
    }
}

/// Walk an expression tree and replace every `NameRef::Unresolved(name)`
/// with `NameRef::Resolved(binding_id, name)`. Used by `catch |e|`
/// desugaring to wire the new match-arm binding to the body's references.
///
/// This function matches every `Expr` variant (enforced by
/// `test_every_expr_variant_handled_by_desugar`). Each arm recurses into
/// child expressions; there is no shorter correct implementation.
#[allow(clippy::too_many_lines)]
pub(crate) fn replace_unresolved_name(expr: &mut Expr, name: &str, binding_id: HirId) {
    match expr {
        Expr::Path(p) => {
            if let NameRef::Unresolved(ref u) = p.name_ref {
                if u.text == name {
                    p.name_ref = NameRef::resolved(binding_id, name);
                }
            }
        }
        Expr::Bin(e) => {
            replace_unresolved_name(&mut e.left, name, binding_id);
            replace_unresolved_name(&mut e.right, name, binding_id);
        }
        Expr::Unary(e) => {
            replace_unresolved_name(&mut e.operand, name, binding_id);
        }
        Expr::Call(e) => {
            for arg in &mut e.args {
                replace_unresolved_name(arg, name, binding_id);
            }
        }
        Expr::MethodCall(e) => {
            replace_unresolved_name(&mut e.receiver, name, binding_id);
            for arg in &mut e.args {
                replace_unresolved_name(arg, name, binding_id);
            }
        }
        Expr::Field(e) => {
            replace_unresolved_name(&mut e.receiver, name, binding_id);
        }
        Expr::Index(e) => {
            replace_unresolved_name(&mut e.base, name, binding_id);
            for idx in &mut e.indices {
                replace_unresolved_name(idx, name, binding_id);
            }
        }
        Expr::Block(e) => {
            for stmt in &mut e.stmts {
                replace_unresolved_name_in_stmt(stmt, name, binding_id);
            }
            if let Some(ref mut tail) = e.tail {
                replace_unresolved_name(tail, name, binding_id);
            }
        }
        Expr::If(e) => {
            replace_unresolved_name(&mut e.condition, name, binding_id);
            replace_unresolved_name_in_block(&mut e.then_branch, name, binding_id);
            if let Some(ref mut els) = e.else_branch {
                replace_unresolved_name(els, name, binding_id);
            }
        }
        Expr::Match(e) => {
            replace_unresolved_name(&mut e.scrutinee, name, binding_id);
            for arm in &mut e.arms {
                if let Some(ref mut guard) = arm.guard {
                    replace_unresolved_name(guard, name, binding_id);
                }
                replace_unresolved_name(&mut arm.body, name, binding_id);
            }
        }
        Expr::Loop(e) => match &mut e.kind {
            LoopKind::Infinite(b) => replace_unresolved_name_in_block(b, name, binding_id),
            LoopKind::Conditional { condition, body } => {
                replace_unresolved_name(condition, name, binding_id);
                replace_unresolved_name_in_block(body, name, binding_id);
            }
            LoopKind::Iterator { iterable, body, .. } => {
                replace_unresolved_name(iterable, name, binding_id);
                replace_unresolved_name_in_block(body, name, binding_id);
            }
        },
        Expr::StructLit(e) => {
            for field in &mut e.fields {
                replace_unresolved_name(&mut field.value, name, binding_id);
            }
        }
        Expr::Assign(e) => {
            replace_unresolved_name_in_assign_target(&mut e.target, name, binding_id);
            replace_unresolved_name(&mut e.value, name, binding_id);
        }
        Expr::Question(e) => {
            replace_unresolved_name(&mut e.expr, name, binding_id);
        }
        Expr::Catch(_) | Expr::Else(_) | Expr::ListLit(_) => {
            // Catch/Else are desugared; ListLit may fall through when lang items are missing.
        }
        Expr::Lit(_) => {}
    }
}

pub(crate) fn replace_unresolved_name_in_stmt(stmt: &mut Stmt, name: &str, binding_id: HirId) {
    match stmt {
        Stmt::ValStmt(s) => replace_unresolved_name(&mut s.value, name, binding_id),
        Stmt::VarStmt(s) => replace_unresolved_name(&mut s.value, name, binding_id),
        Stmt::ExprStmt(s) => replace_unresolved_name(&mut s.expr, name, binding_id),
        Stmt::ReturnStmt(s) => {
            if let Some(ref mut v) = s.value {
                replace_unresolved_name(v, name, binding_id);
            }
        }
        Stmt::BreakStmt(s) => {
            if let Some(ref mut v) = s.value {
                replace_unresolved_name(v, name, binding_id);
            }
        }
        _ => {}
    }
}

pub(crate) fn replace_unresolved_name_in_block(block: &mut Block, name: &str, binding_id: HirId) {
    for stmt in &mut block.stmts {
        replace_unresolved_name_in_stmt(stmt, name, binding_id);
    }
    if let Some(ref mut tail) = block.tail {
        replace_unresolved_name(tail, name, binding_id);
    }
}

pub(crate) fn replace_unresolved_name_in_assign_target(
    target: &mut AssignTarget,
    name: &str,
    binding_id: HirId,
) {
    match target {
        AssignTarget::Name(_) => {}
        AssignTarget::Field { receiver, .. } => {
            replace_unresolved_name(receiver, name, binding_id);
        }
        AssignTarget::Index { base, indices } => {
            replace_unresolved_name(base, name, binding_id);
            for idx in indices {
                replace_unresolved_name(idx, name, binding_id);
            }
        }
    }
}
