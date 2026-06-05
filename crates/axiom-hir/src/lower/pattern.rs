//! Pattern lowering: CST pattern nodes → HIR `Pattern`.

use super::{path_last_segment, token_text, Def, DefKind, LowerCtx};
use crate::hir::*;
use crate::HirDiagnostic;
use axiom_parser::ast::{self, AstNode};

pub(super) fn lower_pattern(node: &axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> Pattern {
    if let Some(p) = ast::WildcardPat::cast(node.clone()) {
        let _ = p;
        Pattern::Wildcard(ctx.alloc_id())
    } else if let Some(p) = ast::IdentPat::cast(node.clone()) {
        lower_ident_pattern(&p, ctx)
    } else if let Some(p) = ast::LiteralPat::cast(node.clone()) {
        lower_literal_pattern(&p, ctx)
    } else if let Some(p) = ast::TupleStructPat::cast(node.clone()) {
        lower_tuple_struct_pattern(&p, ctx)
    } else if let Some(p) = ast::StructPat::cast(node.clone()) {
        lower_struct_pattern(&p, ctx)
    } else if let Some(p) = ast::OrPat::cast(node.clone()) {
        lower_or_pattern(&p, ctx)
    } else if let Some(p) = ast::RangePat::cast(node.clone()) {
        lower_range_pattern(&p, ctx)
    } else {
        let id = ctx.alloc_id();
        ctx.diag(HirDiagnostic::NotYetSupported {
            feature: format!("pattern kind {:?}", node.kind()),
            span: ctx.span_of(node),
        });
        Pattern::Wildcard(id)
    }
}

pub(super) fn lower_pattern_from_let(let_stmt: &ast::LetStmt, ctx: &mut LowerCtx) -> Pattern {
    let pat_node = ast::child_pat_node(let_stmt.syntax());
    pat_node
        .map(|n| lower_pattern(&n, ctx))
        .unwrap_or_else(|| Pattern::Wildcard(ctx.alloc_id()))
}

fn lower_ident_pattern(p: &ast::IdentPat, ctx: &mut LowerCtx) -> Pattern {
    let id = ctx.alloc_id();
    // IdentPat contains a Path; the name is the last segment's identifier.
    let name = p
        .syntax()
        .child_nodes()
        .into_iter()
        .find_map(|n| {
            ast::Path::cast(n).and_then(|path| {
                path.segments()
                    .into_iter()
                    .last()
                    .and_then(|seg| seg.name_token())
            })
        })
        .map(|t| t.text().to_string())
        .unwrap_or_default();
    ctx.defs.push(Def {
        name: name.clone(),
        def_id: id,
        kind: DefKind::Local,
        span: ctx.span_of(p.syntax()),
    });
    Pattern::Ident(IdentPat {
        id,
        name,
        binding: Some(id),
        span: ctx.span_of(p.syntax()),
    })
}

fn lower_literal_pattern(p: &ast::LiteralPat, ctx: &mut LowerCtx) -> Pattern {
    let id = ctx.alloc_id();
    let kind = p
        .token()
        .map(|t| super::lit_kind_from_token(&t))
        .unwrap_or(LitKind::Unit);
    Pattern::Literal(LitPat { id, kind })
}

fn lower_tuple_struct_pattern(p: &ast::TupleStructPat, ctx: &mut LowerCtx) -> Pattern {
    let id = ctx.alloc_id();
    let path = p
        .path()
        .map(|path_node| NameRef::unresolved(path_last_segment(Some(path_node))))
        .unwrap_or_else(|| NameRef::unresolved(""));
    let fields = p
        .fields()
        .map(|fl| fl.patterns())
        .unwrap_or_default()
        .into_iter()
        .map(|n| lower_pattern(&n, ctx))
        .collect::<Vec<_>>();
    Pattern::TupleStruct(TupleStructPat { id, path, fields })
}

fn lower_struct_pattern(p: &ast::StructPat, ctx: &mut LowerCtx) -> Pattern {
    let id = ctx.alloc_id();
    let path = p
        .path()
        .map(|path_node| NameRef::unresolved(path_last_segment(Some(path_node))))
        .unwrap_or_else(|| NameRef::unresolved(""));
    let fields = p
        .field_list()
        .map(|fl| {
            fl.fields()
                .into_iter()
                .map(|f| {
                    let name = token_text(f.name_token());
                    let pattern = f
                        .pattern()
                        .map(|pat| lower_pattern(&pat, ctx))
                        .unwrap_or_else(|| Pattern::Wildcard(ctx.alloc_id()));
                    StructPatField { name, pattern }
                })
                .collect()
        })
        .unwrap_or_default();
    Pattern::Struct(StructPat { id, path, fields })
}

fn lower_or_pattern(p: &ast::OrPat, ctx: &mut LowerCtx) -> Pattern {
    let id = ctx.alloc_id();
    let alternatives = p
        .alternatives()
        .into_iter()
        .map(|alt| lower_pattern(&alt, ctx))
        .collect();
    Pattern::Or(OrPat { id, alternatives })
}

fn lower_range_pattern(p: &ast::RangePat, ctx: &mut LowerCtx) -> Pattern {
    let id = ctx.alloc_id();
    let start = p.start_literal().map(|t| super::lit_kind_from_token(&t));
    let end = p.end_literal().map(|t| super::lit_kind_from_token(&t));
    let inclusive = p
        .range_op_token()
        .map(|t| t.text() == "..=")
        .unwrap_or(true);
    Pattern::Range(RangePat {
        id,
        start,
        end,
        inclusive,
    })
}
