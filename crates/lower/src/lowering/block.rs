//! Block and statement lowering.

use super::expr::{lower_expr, unit_expr};
use super::pattern::lower_pattern_from_let;
use super::token_text;
use super::ty::lower_ty;
use super::LowerCtx;
use crate::hir_types::*;
use crate::HirDiagnostic;
use parser::ast::{self, AstNode};
use parser::SyntaxKind;

pub(super) fn lower_block(block: &ast::BlockExpr, ctx: &mut LowerCtx) -> Block {
    let id = ctx.alloc_id();
    let raw_stmts = block.stmts();
    let mut stmts = Vec::new();
    for child in &raw_stmts {
        if let Some(stmt) = lower_stmt(child.clone(), ctx) {
            stmts.push(stmt);
        }
    }
    // The last ExprStmt in a block is the tail expression — its value is the
    // block's result — UNLESS it ends with a trailing `;`, which discards the
    // value so the block evaluates to `()` (DESIGN_SPEC §16). A bare expression
    // node (no statement wrapper) is always a tail.
    let tail = match stmts.last() {
        Some(Stmt::ExprStmt(_)) => {
            let is_tail = match raw_stmts.last() {
                Some(n) if n.kind() == SyntaxKind::ExprStmt => ast::ExprStmt::cast(n.clone())
                    .map(|s| !s.has_semicolon())
                    .unwrap_or(false),
                Some(n) => ast::is_expr_kind(n.kind()),
                None => false,
            };
            if is_tail {
                match stmts.pop() {
                    Some(Stmt::ExprStmt(e)) => Some(Box::new(e.expr)),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    };
    Block { id, stmts, tail }
}

fn lower_stmt(child: parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Stmt> {
    let kind = child.kind();
    if kind == SyntaxKind::LetStmt {
        lower_let_stmt(child, ctx)
    } else if kind == SyntaxKind::ReturnStmt {
        lower_return_stmt(child, ctx)
    } else if kind == SyntaxKind::ErrdeferStmt {
        ctx.diag(HirDiagnostic::NotYetSupported {
            feature: "errdefer".to_string(),
            span: ctx.span_of(&child),
        });
        None
    } else if kind == SyntaxKind::YieldStmt {
        lower_yield_stmt(child, ctx)
    } else if kind == SyntaxKind::Error {
        None
    } else if ast::is_expr_kind(kind) || kind == SyntaxKind::ExprStmt {
        lower_expr_stmt(child, ctx)
    } else {
        None
    }
}

fn lower_let_stmt(child: parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Stmt> {
    let let_stmt = ast::LetStmt::cast(child)?;
    let kw_text = token_text(let_stmt.binding_kw());
    let pattern = lower_pattern_from_let(&let_stmt, ctx);
    let ty = let_stmt.ty().map(|ty_node| lower_ty(&ty_node, ctx));
    let value = let_stmt
        .value()
        .map(|e| lower_expr(&e, ctx))
        .unwrap_or_else(|| unit_expr(ctx));
    let stmt = if kw_text == "var" {
        Stmt::VarStmt(VarStmt {
            id: ctx.alloc_id(),
            pattern,
            ty,
            value,
        })
    } else {
        Stmt::ValStmt(ValStmt {
            id: ctx.alloc_id(),
            pattern,
            ty,
            value,
        })
    };
    Some(stmt)
}

fn lower_return_stmt(child: parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Stmt> {
    let ret_stmt = ast::ReturnStmt::cast(child)?;
    let stmt_id = ctx.alloc_id();
    let value = ret_stmt.value().map(|v| lower_expr(&v, ctx));
    Some(Stmt::ReturnStmt(ReturnStmt { id: stmt_id, value }))
}

fn lower_yield_stmt(child: parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Stmt> {
    let yield_stmt = ast::YieldStmt::cast(child)?;
    let stmt_id = ctx.alloc_id();
    let value = yield_stmt
        .value()
        .map(|v| lower_expr(&v, ctx))
        .unwrap_or_else(|| unit_expr(ctx));
    Some(Stmt::YieldStmt(YieldStmt { id: stmt_id, value }))
}

fn lower_expr_stmt(child: parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Stmt> {
    if let Some(expr_stmt) = ast::ExprStmt::cast(child.clone()) {
        if let Some(e) = expr_stmt.expr() {
            let stmt_id = ctx.alloc_id();
            Some(Stmt::ExprStmt(ExprStmt {
                id: stmt_id,
                expr: lower_expr(&e, ctx),
            }))
        } else {
            None
        }
    } else if ast::is_expr_kind(child.kind()) {
        let stmt_id = ctx.alloc_id();
        Some(Stmt::ExprStmt(ExprStmt {
            id: stmt_id,
            expr: lower_expr(&child, ctx),
        }))
    } else {
        None
    }
}
