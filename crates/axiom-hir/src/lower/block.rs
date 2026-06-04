//! Block and statement lowering.

use super::expr::{lower_expr, unit_expr};
use super::pattern::lower_pattern_from_let;
use super::token_text;
use super::ty::lower_ty;
use super::LowerCtx;
use crate::hir::*;
use crate::HirDiagnostic;
use axiom_parser::ast::{self, AstNode};
use axiom_parser::SyntaxKind;

pub(super) fn lower_block(block: &ast::BlockExpr, ctx: &mut LowerCtx) -> Block {
    let id = ctx.alloc_id();
    let mut stmts = Vec::new();
    for child in block.stmts() {
        if let Some(stmt) = lower_stmt(child, ctx) {
            stmts.push(stmt);
        }
    }
    Block {
        id,
        stmts,
        tail: None,
    }
}

fn lower_stmt(child: axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Stmt> {
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
    } else if kind == SyntaxKind::Error {
        None
    } else if ast::is_expr_kind(kind) || kind == SyntaxKind::ExprStmt {
        lower_expr_stmt(child, ctx)
    } else {
        None
    }
}

fn lower_let_stmt(child: axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Stmt> {
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

fn lower_return_stmt(child: axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Stmt> {
    let ret_stmt = ast::ReturnStmt::cast(child)?;
    let stmt_id = ctx.alloc_id();
    let value = ret_stmt.value().map(|v| lower_expr(&v, ctx));
    Some(Stmt::ReturnStmt(ReturnStmt { id: stmt_id, value }))
}

fn lower_expr_stmt(child: axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Stmt> {
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
