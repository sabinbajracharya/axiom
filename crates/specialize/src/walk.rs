//! Expression and statement walking for monomorphization.
//!
//! Two modes: plain walk (for non-generic entry functions) and
//! substitution-aware walk (for generic function bodies being specialized).

use resolver::{Block, Expr, LoopKind, Stmt};

use crate::helpers::Substitution;
use crate::mono::Monomorphizer;

impl<'a> Monomorphizer<'a> {
    // ── Plain walk ─────────────────────────────────────────────────────────

    pub(super) fn collect_from_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.collect_from_stmt(stmt);
        }
        if let Some(tail) = &block.tail {
            self.collect_from_expr(tail);
        }
    }

    fn collect_from_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::ValStmt(s) => self.collect_from_expr(&s.value),
            Stmt::VarStmt(s) => self.collect_from_expr(&s.value),
            Stmt::ExprStmt(s) => self.collect_from_expr(&s.expr),
            Stmt::ReturnStmt(s) => {
                if let Some(v) = &s.value {
                    self.collect_from_expr(v);
                }
            }
            Stmt::BreakStmt(s) => {
                if let Some(v) = &s.value {
                    self.collect_from_expr(v);
                }
            }
            Stmt::ContinueStmt(_) => {}
            Stmt::YieldStmt(s) => self.collect_from_expr(&s.value),
        }
    }

    fn collect_from_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Call(call) => {
                self.visit_call(call);
                for arg in &call.args {
                    self.collect_from_expr(arg);
                }
            }
            Expr::MethodCall(mc) => {
                self.collect_from_expr(&mc.receiver);
                for arg in &mc.args {
                    self.collect_from_expr(arg);
                }
            }
            Expr::Bin(b) => {
                self.collect_from_expr(&b.left);
                self.collect_from_expr(&b.right);
            }
            Expr::Unary(u) => self.collect_from_expr(&u.operand),
            Expr::Field(f) => self.collect_from_expr(&f.receiver),
            Expr::Index(i) => {
                self.collect_from_expr(&i.base);
                for index in &i.indices {
                    self.collect_from_expr(index);
                }
            }
            Expr::Block(b) => self.collect_from_block(b),
            Expr::If(i) => {
                self.collect_from_expr(&i.condition);
                self.collect_from_block(&i.then_branch);
                if let Some(else_b) = &i.else_branch {
                    self.collect_from_expr(else_b);
                }
            }
            Expr::Match(m) => {
                self.collect_from_expr(&m.scrutinee);
                for arm in &m.arms {
                    self.collect_from_expr(&arm.body);
                }
            }
            Expr::Loop(l) => match &l.kind {
                LoopKind::Infinite(b) => self.collect_from_block(b),
                LoopKind::Conditional { condition, body } => {
                    self.collect_from_expr(condition);
                    self.collect_from_block(body);
                }
                LoopKind::Iterator { iterable, body, .. } => {
                    self.collect_from_expr(iterable);
                    self.collect_from_block(body);
                }
            },
            Expr::StructLit(s) => {
                for f in &s.fields {
                    self.collect_from_expr(&f.value);
                }
            }
            Expr::Assign(a) => self.collect_from_expr(&a.value),
            Expr::ListLit(l) => l.elements.iter().for_each(|e| self.collect_from_expr(e)),
            Expr::Lit(_) | Expr::Path(_) => {}
        }
    }

    // ── Substitution-aware walk ────────────────────────────────────────────

    pub(super) fn collect_from_block_with_subst(&mut self, block: &Block, subst: &Substitution) {
        for stmt in &block.stmts {
            self.collect_from_stmt_with_subst(stmt, subst);
        }
        if let Some(tail) = &block.tail {
            self.collect_from_expr_with_subst(tail, subst);
        }
    }

    fn collect_from_stmt_with_subst(&mut self, stmt: &Stmt, subst: &Substitution) {
        match stmt {
            Stmt::ValStmt(s) => {
                self.collect_from_expr_with_subst(&s.value, subst);
            }
            Stmt::VarStmt(s) => {
                self.collect_from_expr_with_subst(&s.value, subst);
            }
            Stmt::ExprStmt(s) => {
                self.collect_from_expr_with_subst(&s.expr, subst);
            }
            Stmt::ReturnStmt(s) => {
                if let Some(v) = &s.value {
                    self.collect_from_expr_with_subst(v, subst);
                }
            }
            Stmt::BreakStmt(s) => {
                if let Some(v) = &s.value {
                    self.collect_from_expr_with_subst(v, subst);
                }
            }
            Stmt::ContinueStmt(_) => {}
            Stmt::YieldStmt(s) => self.collect_from_expr_with_subst(&s.value, subst),
        }
    }

    fn collect_from_expr_with_subst(&mut self, expr: &Expr, subst: &Substitution) {
        match expr {
            Expr::Call(call) => {
                self.visit_call_with_subst(call, subst);
                call.args
                    .iter()
                    .for_each(|arg| self.collect_from_expr_with_subst(arg, subst));
            }
            Expr::MethodCall(mc) => {
                self.collect_from_expr_with_subst(&mc.receiver, subst);
                mc.args
                    .iter()
                    .for_each(|arg| self.collect_from_expr_with_subst(arg, subst));
            }
            Expr::Bin(b) => {
                self.collect_from_expr_with_subst(&b.left, subst);
                self.collect_from_expr_with_subst(&b.right, subst);
            }
            Expr::Unary(u) => self.collect_from_expr_with_subst(&u.operand, subst),
            Expr::Field(f) => self.collect_from_expr_with_subst(&f.receiver, subst),
            Expr::Index(i) => {
                self.collect_from_expr_with_subst(&i.base, subst);
                i.indices
                    .iter()
                    .for_each(|idx| self.collect_from_expr_with_subst(idx, subst));
            }
            Expr::Block(b) => self.collect_from_block_with_subst(b, subst),
            Expr::If(i) => {
                self.collect_from_expr_with_subst(&i.condition, subst);
                self.collect_from_block_with_subst(&i.then_branch, subst);
                if let Some(else_b) = &i.else_branch {
                    self.collect_from_expr_with_subst(else_b, subst);
                }
            }
            Expr::Match(m) => {
                self.collect_from_expr_with_subst(&m.scrutinee, subst);
                m.arms
                    .iter()
                    .for_each(|arm| self.collect_from_expr_with_subst(&arm.body, subst));
            }
            Expr::Loop(l) => self.collect_from_loop_with_subst(&l.kind, subst),
            Expr::StructLit(s) => {
                s.fields
                    .iter()
                    .for_each(|f| self.collect_from_expr_with_subst(&f.value, subst));
            }
            Expr::Assign(a) => self.collect_from_expr_with_subst(&a.value, subst),
            Expr::ListLit(l) => {
                l.elements
                    .iter()
                    .for_each(|elem| self.collect_from_expr_with_subst(elem, subst));
            }
            Expr::Lit(_) | Expr::Path(_) => {}
        }
    }

    fn collect_from_loop_with_subst(&mut self, kind: &LoopKind, subst: &Substitution) {
        match kind {
            LoopKind::Infinite(b) => {
                self.collect_from_block_with_subst(b, subst);
            }
            LoopKind::Conditional { condition, body } => {
                self.collect_from_expr_with_subst(condition, subst);
                self.collect_from_block_with_subst(body, subst);
            }
            LoopKind::Iterator { iterable, body, .. } => {
                self.collect_from_expr_with_subst(iterable, subst);
                self.collect_from_block_with_subst(body, subst);
            }
        }
    }
}
