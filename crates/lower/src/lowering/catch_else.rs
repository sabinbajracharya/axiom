//! Lowering for `catch` and `else` expressions. Extracted from `expr.rs`
//! to stay under the 600-line cap (RUST_CONVENTIONS.md §10).

use super::LowerCtx;
use crate::hir_types::*;
use crate::lowering::expr::{lower_expr, unit_expr};
use parser::ast::{self, AstNode};

pub(crate) fn lower_catch_expr(e: &ast::CatchExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let expr = e
        .expr()
        .map(|node| lower_expr(&node, ctx))
        .unwrap_or_else(|| unit_expr(ctx));
    let handler_node = e.handler();
    let (fallback, error_binding, error_binding_id) =
        extract_closure_capture(handler_node.as_ref(), ctx);
    Expr::Catch(CatchExpr {
        id,
        expr: Box::new(expr),
        fallback,
        error_binding,
        error_binding_id,
    })
}

pub(crate) fn lower_else_expr(e: &ast::ElseExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let expr = e
        .expr()
        .map(|node| lower_expr(&node, ctx))
        .unwrap_or_else(|| unit_expr(ctx));
    let fallback = e
        .handler()
        .map(|node| lower_expr(&node, ctx))
        .unwrap_or_else(|| unit_expr(ctx));
    Expr::Else(ElseExpr {
        id,
        expr: Box::new(expr),
        fallback: Box::new(fallback),
    })
}

/// If `handler_node` is a single-param closure `|name| body`, return
/// `(lowered_body, Some(name), Some(binding_id))`.
/// Otherwise return `(lowered_node, None, None)`.
fn extract_closure_capture(
    handler_node: Option<&parser::SyntaxNode>,
    ctx: &mut LowerCtx,
) -> (Box<Expr>, Option<String>, Option<HirId>) {
    let Some(node) = handler_node else {
        return (Box::new(unit_expr(ctx)), None, None);
    };
    let Some(closure) = ast::ClosureExpr::cast(node.clone()) else {
        let fb = lower_expr(node, ctx);
        return (Box::new(fb), None, None);
    };
    let Some(pl) = closure.param_list() else {
        let fb = lower_expr(node, ctx);
        return (Box::new(fb), None, None);
    };
    let params = pl.params();
    if params.len() != 1 || params[0].has_type_annotation() {
        let fb = lower_expr(node, ctx);
        return (Box::new(fb), None, None);
    }
    let name = params[0]
        .name()
        .map(|n| n.text().to_string())
        .unwrap_or_default();
    if name.is_empty() {
        let fb = lower_expr(node, ctx);
        return (Box::new(fb), None, None);
    }
    let binding_id = ctx.alloc_id();
    let body = closure
        .body()
        .map(|b| lower_expr(&b, ctx))
        .unwrap_or_else(|| unit_expr(ctx));
    (Box::new(body), Some(name), Some(binding_id))
}
