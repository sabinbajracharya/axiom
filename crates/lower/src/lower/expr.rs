//! Expression lowering: CST expression nodes → HIR `Expr`.

use super::block::lower_block;
use super::pattern::lower_pattern;
use super::{path_last_segment, path_last_segment_from_node, path_qualifier_from_node, LowerCtx};
use crate::hir::*;
use crate::HirDiagnostic;
use axiom_parser::ast::{self, AstNode};
use axiom_parser::SyntaxKind;

pub(super) fn lower_expr(node: &axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> Expr {
    if let Some(e) = ast::LiteralExpr::cast(node.clone()) {
        return lower_literal(&e, ctx);
    }
    if let Some(e) = ast::PathExpr::cast(node.clone()) {
        return lower_path_expr(&e, ctx);
    }
    if let Some(e) = ast::BinExpr::cast(node.clone()) {
        return lower_bin_expr(&e, ctx);
    }
    if let Some(e) = ast::PrefixExpr::cast(node.clone()) {
        return lower_prefix_expr(&e, ctx);
    }
    if let Some(e) = ast::CallExpr::cast(node.clone()) {
        return lower_call_expr(&e, ctx);
    }
    if let Some(e) = ast::MethodCallExpr::cast(node.clone()) {
        return lower_method_call_expr(&e, ctx);
    }
    if let Some(e) = ast::FieldExpr::cast(node.clone()) {
        return lower_field_expr(&e, ctx);
    }
    if let Some(e) = ast::IndexExpr::cast(node.clone()) {
        return lower_index_expr(&e, ctx);
    }
    if let Some(e) = ast::BlockExpr::cast(node.clone()) {
        return Expr::Block(lower_block(&e, ctx));
    }
    if let Some(e) = ast::ParenExpr::cast(node.clone()) {
        return lower_paren_expr(&e, ctx);
    }
    if let Some(e) = ast::IfExpr::cast(node.clone()) {
        return lower_if_expr(&e, ctx);
    }
    if let Some(e) = ast::MatchExpr::cast(node.clone()) {
        return lower_match_expr(&e, ctx);
    }
    if let Some(e) = ast::LoopExpr::cast(node.clone()) {
        return lower_loop_expr(&e, ctx);
    }
    if let Some(e) = ast::StructLitExpr::cast(node.clone()) {
        return lower_struct_lit_expr(&e, ctx);
    }
    if let Some(e) = ast::AssignExpr::cast(node.clone()) {
        return lower_assign_expr(&e, ctx);
    }
    if let Some(e) = ast::ListLitExpr::cast(node.clone()) {
        return lower_list_lit_expr(&e, ctx);
    }
    if let Some(result) = lower_stmt_expr(node, ctx) {
        return result;
    }
    unsupported_expr(ctx, &format!("expression kind {:?}", node.kind()), node)
}

pub(super) fn unit_expr(ctx: &mut LowerCtx) -> Expr {
    Expr::Lit(LitExpr {
        id: ctx.alloc_id(),
        kind: LitKind::Unit,
    })
}

fn unit_block(ctx: &mut LowerCtx) -> Block {
    Block {
        id: ctx.alloc_id(),
        stmts: Vec::new(),
        tail: None,
    }
}

fn unsupported_expr(ctx: &mut LowerCtx, feature: &str, node: &axiom_parser::SyntaxNode) -> Expr {
    let id = ctx.alloc_id();
    ctx.diag(HirDiagnostic::NotYetSupported {
        feature: feature.to_string(),
        span: ctx.span_of(node),
    });
    Expr::Lit(LitExpr {
        id,
        kind: LitKind::Unit,
    })
}

fn lower_literal(e: &ast::LiteralExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let kind = e
        .token()
        .map(|tok| super::lit_kind_from_token(&tok))
        .unwrap_or(LitKind::Unit);
    Expr::Lit(LitExpr { id, kind })
}

fn lower_path_expr(e: &ast::PathExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let name = path_last_segment(e.path());
    Expr::Path(PathExpr {
        id,
        name_ref: NameRef::unresolved(name),
    })
}

fn lower_bin_expr(e: &ast::BinExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let op = e
        .op_token()
        .map(|t| bin_op_from_token(t.kind()))
        .unwrap_or(BinOp::Add);
    let left = e
        .lhs()
        .map(|n| Box::new(lower_expr(&n, ctx)))
        .unwrap_or_else(|| Box::new(unit_expr(ctx)));
    let right = e
        .rhs()
        .map(|n| Box::new(lower_expr(&n, ctx)))
        .unwrap_or_else(|| Box::new(unit_expr(ctx)));
    Expr::Bin(BinExpr {
        id,
        op,
        left,
        right,
    })
}

fn lower_prefix_expr(e: &ast::PrefixExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let op = e
        .op_token()
        .map(|t| match t.kind() {
            SyntaxKind::Minus => UnaryOp::Neg,
            SyntaxKind::Bang => UnaryOp::Not,
            _ => UnaryOp::Neg,
        })
        .unwrap_or(UnaryOp::Neg);
    let operand = e
        .expr()
        .map(|n| Box::new(lower_expr(&n, ctx)))
        .unwrap_or_else(|| Box::new(unit_expr(ctx)));
    Expr::Unary(UnaryExpr { id, op, operand })
}

fn lower_call_expr(e: &ast::CallExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let callee_node = e.callee();
    let callee = callee_node
        .as_ref()
        .and_then(path_last_segment_from_node)
        .unwrap_or_default();
    let qualifier = callee_node.as_ref().and_then(path_qualifier_from_node);
    let args = e
        .arg_list()
        .map(|al| al.args().into_iter().map(|a| lower_expr(&a, ctx)).collect())
        .unwrap_or_default();
    Expr::Call(CallExpr {
        id,
        callee: NameRef::unresolved(callee),
        qualifier,
        args,
    })
}

fn lower_method_call_expr(e: &ast::MethodCallExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let receiver = e
        .receiver()
        .map(|n| Box::new(lower_expr(&n, ctx)))
        .unwrap_or_else(|| Box::new(unit_expr(ctx)));
    let method = e.method_name().and_then(|nr| nr.text()).unwrap_or_default();
    let args = e
        .arg_list()
        .map(|al| al.args().into_iter().map(|a| lower_expr(&a, ctx)).collect())
        .unwrap_or_default();
    Expr::MethodCall(MethodCallExpr {
        id,
        receiver,
        method,
        args,
    })
}

fn lower_field_expr(e: &ast::FieldExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let receiver = e
        .expr()
        .map(|n| Box::new(lower_expr(&n, ctx)))
        .unwrap_or_else(|| Box::new(unit_expr(ctx)));
    let field = e.field_name().and_then(|nr| nr.text()).unwrap_or_default();
    Expr::Field(FieldExpr {
        id,
        receiver,
        field,
    })
}

fn lower_index_expr(e: &ast::IndexExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let base = e
        .base()
        .map(|n| Box::new(lower_expr(&n, ctx)))
        .unwrap_or_else(|| Box::new(unit_expr(ctx)));
    let indices: Vec<Expr> = e
        .indices()
        .into_iter()
        .map(|n| lower_expr(&n, ctx))
        .collect();
    Expr::Index(IndexExpr { id, base, indices })
}

fn lower_paren_expr(e: &ast::ParenExpr, ctx: &mut LowerCtx) -> Expr {
    e.expr()
        .map(|inner| lower_expr(&inner, ctx))
        .unwrap_or_else(|| unit_expr(ctx))
}

fn lower_if_expr(e: &ast::IfExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let condition = e
        .condition()
        .map(|n| Box::new(lower_expr(&n, ctx)))
        .unwrap_or_else(|| {
            Box::new(Expr::Lit(LitExpr {
                id: ctx.alloc_id(),
                kind: LitKind::Bool(true),
            }))
        });
    let then_branch = e
        .then_branch()
        .map(|b| lower_block(&b, ctx))
        .unwrap_or_else(|| unit_block(ctx));
    let else_branch = e.else_branch().map(|n| Box::new(lower_expr(&n, ctx)));
    Expr::If(IfExpr {
        id,
        condition,
        then_branch,
        else_branch,
    })
}

fn lower_match_expr(e: &ast::MatchExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let scrutinee = e
        .scrutinee()
        .map(|n| Box::new(lower_expr(&n, ctx)))
        .unwrap_or_else(|| Box::new(unit_expr(ctx)));
    let arms = e
        .arm_list()
        .map(|al| al.arms())
        .unwrap_or_default()
        .into_iter()
        .map(|arm| {
            let pattern = arm
                .pattern()
                .map(|p| lower_pattern(&p, ctx))
                .unwrap_or_else(|| Pattern::Wildcard(ctx.alloc_id()));
            let guard = arm
                .guard()
                .and_then(|g| g.expr())
                .map(|g| lower_expr(&g, ctx));
            let body = arm
                .body()
                .map(|b| lower_expr(&b, ctx))
                .unwrap_or_else(|| unit_expr(ctx));
            MatchArm {
                pattern,
                guard,
                body,
            }
        })
        .collect();
    Expr::Match(MatchExpr {
        id,
        scrutinee,
        arms,
    })
}

fn lower_loop_expr(e: &ast::LoopExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let body = e
        .body()
        .map(|b| lower_block(&b, ctx))
        .unwrap_or_else(|| unit_block(ctx));
    let kind = if e.is_iterator() {
        let binding = e
            .iter_pattern()
            .and_then(ast::IdentPat::cast)
            .and_then(|ip| ip.name_token())
            .map(|t| t.text().to_string())
            .unwrap_or_default();
        let binding_id = ctx.alloc_id();
        let iterable = e
            .iter_iterable()
            .map(|n| Box::new(lower_expr(&n, ctx)))
            .unwrap_or_else(|| Box::new(unit_expr(ctx)));
        LoopKind::Iterator {
            binding,
            binding_id,
            iterable,
            body,
        }
    } else if e.is_conditional() {
        let condition = e
            .loop_condition()
            .map(|n| Box::new(lower_expr(&n, ctx)))
            .unwrap_or_else(|| {
                Box::new(Expr::Lit(LitExpr {
                    id: ctx.alloc_id(),
                    kind: LitKind::Bool(true),
                }))
            });
        LoopKind::Conditional { condition, body }
    } else {
        LoopKind::Infinite(body)
    };
    Expr::Loop(LoopExpr { id, kind })
}

fn lower_list_lit_expr(e: &ast::ListLitExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let elements = e
        .elements()
        .into_iter()
        .map(|elem| lower_expr(&elem, ctx))
        .collect();
    Expr::ListLit(ListLitExpr { id, elements })
}

fn lower_stmt_expr(node: &axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Expr> {
    if let Some(e) = ast::ReturnStmt::cast(node.clone()) {
        return Some(lower_return_as_expr(&e, ctx));
    }
    if let Some(e) = ast::BreakStmt::cast(node.clone()) {
        return Some(lower_break_expr(&e, ctx));
    }
    if ast::ContinueStmt::cast(node.clone()).is_some() {
        let id = ctx.alloc_id();
        let cont = ContinueStmt { id };
        return Some(Expr::Block(Block {
            id: ctx.alloc_id(),
            stmts: vec![Stmt::ContinueStmt(cont)],
            tail: None,
        }));
    }
    None
}

fn lower_struct_lit_expr(e: &ast::StructLitExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let type_name = e
        .path()
        .map(|p| NameRef::unresolved(path_last_segment(Some(p))))
        .unwrap_or_else(|| NameRef::unresolved(""));
    let fields = e
        .field_list()
        .map(|fl| {
            fl.fields()
                .into_iter()
                .map(|f| {
                    let name = super::token_text(f.name_token());
                    let value = f
                        .value()
                        .map(|v| lower_expr(&v, ctx))
                        .unwrap_or_else(|| unit_expr(ctx));
                    StructLitField { name, value }
                })
                .collect()
        })
        .unwrap_or_default();
    Expr::StructLit(StructLitExpr {
        id,
        type_name,
        fields,
    })
}

fn lower_assign_expr(e: &ast::AssignExpr, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let op = e
        .op_token()
        .map(|t| match t.kind() {
            SyntaxKind::Eq => AssignOp::Plain,
            SyntaxKind::PlusEq => AssignOp::Add,
            SyntaxKind::MinusEq => AssignOp::Sub,
            SyntaxKind::StarEq => AssignOp::Mul,
            SyntaxKind::SlashEq => AssignOp::Div,
            SyntaxKind::PercentEq => AssignOp::Mod,
            _ => AssignOp::Plain,
        })
        .unwrap_or(AssignOp::Plain);
    let target = if let Some(lhs) = e.lhs() {
        lower_assign_target(&lhs, ctx)
    } else {
        AssignTarget::Name(NameRef::unresolved(""))
    };
    let value = e
        .rhs()
        .map(|n| Box::new(lower_expr(&n, ctx)))
        .unwrap_or_else(|| Box::new(unit_expr(ctx)));
    Expr::Assign(AssignExpr {
        id,
        target,
        value,
        op,
    })
}

fn lower_assign_target(node: &axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> AssignTarget {
    if let Some(path) = ast::PathExpr::cast(node.clone()) {
        let name = path_last_segment(path.path());
        return AssignTarget::Name(NameRef::unresolved(name));
    }
    if let Some(field) = ast::FieldExpr::cast(node.clone()) {
        let receiver = field
            .expr()
            .map(|n| Box::new(lower_expr(&n, ctx)))
            .unwrap_or_else(|| Box::new(unit_expr(ctx)));
        let field_name = field
            .field_name()
            .and_then(|nr| nr.text())
            .unwrap_or_default();
        return AssignTarget::Field {
            receiver,
            field: field_name,
        };
    }
    if let Some(index) = ast::IndexExpr::cast(node.clone()) {
        let base = index
            .base()
            .map(|n| Box::new(lower_expr(&n, ctx)))
            .unwrap_or_else(|| Box::new(unit_expr(ctx)));
        let indices: Vec<Expr> = index
            .indices()
            .into_iter()
            .map(|n| lower_expr(&n, ctx))
            .collect();
        return AssignTarget::Index { base, indices };
    }
    AssignTarget::Name(NameRef::unresolved(""))
}

fn lower_return_as_expr(e: &ast::ReturnStmt, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let value = e.value().map(|v| lower_expr(&v, ctx));
    Expr::Block(Block {
        id,
        stmts: vec![Stmt::ReturnStmt(ReturnStmt {
            id: ctx.alloc_id(),
            value,
        })],
        tail: None,
    })
}

fn lower_break_expr(e: &ast::BreakStmt, ctx: &mut LowerCtx) -> Expr {
    let id = ctx.alloc_id();
    let value = e.value().map(|v| lower_expr(&v, ctx));
    Expr::Block(Block {
        id,
        stmts: vec![Stmt::BreakStmt(BreakStmt {
            id: ctx.alloc_id(),
            value,
        })],
        tail: None,
    })
}

fn bin_op_from_token(kind: SyntaxKind) -> BinOp {
    use SyntaxKind::*;
    match kind {
        Plus => BinOp::Add,
        Minus => BinOp::Sub,
        Star => BinOp::Mul,
        Slash => BinOp::Div,
        Percent => BinOp::Mod,
        EqEq => BinOp::Eq,
        Ne => BinOp::Ne,
        Lt => BinOp::Lt,
        Le => BinOp::Le,
        Gt => BinOp::Gt,
        Ge => BinOp::Ge,
        AmpAmp => BinOp::And,
        PipePipe => BinOp::Or,
        Shl => BinOp::Shl,
        Shr => BinOp::Shr,
        Amp => BinOp::BitAnd,
        Pipe => BinOp::BitOr,
        Caret => BinOp::BitXor,
        _ => BinOp::Add,
    }
}
