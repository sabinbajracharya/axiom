//! Error-set and error-handling lowering: CST → HIR for `error` definitions,
//! `try` expressions, and `else` expressions.

use super::{name_text, token_text, Def, DefKind, LowerCtx};
use crate::hir_types::*;
use parser::ast::{self, AstNode};

pub(super) fn lower_error_set_def(e: &ast::ErrorSetDef, ctx: &mut LowerCtx) -> ErrorSetDef {
    let id = ctx.alloc_id();
    let name = e.name().map(|n| name_text(&n)).unwrap_or_default();
    let visibility = if e.visibility().is_some() {
        Visibility::Public
    } else {
        Visibility::Private
    };
    let variants = e
        .variant_list()
        .map(|vl| vl.variants())
        .unwrap_or_default()
        .into_iter()
        .map(|v| {
            let vid = ctx.alloc_id();
            let vname = token_text(v.name_token());
            ctx.defs.push(Def {
                name: vname.clone(),
                def_id: vid,
                kind: DefKind::ErrorVariant,
                visibility,
                span: ctx.span_of(v.syntax()),
            });
            ErrorVariantDef {
                id: vid,
                name: vname,
            }
        })
        .collect();

    ctx.defs.push(Def {
        name: name.clone(),
        def_id: id,
        kind: DefKind::ErrorSet,
        visibility,
        span: ctx.span_of(e.syntax()),
    });

    ErrorSetDef {
        id,
        name,
        visibility,
        variants,
    }
}
