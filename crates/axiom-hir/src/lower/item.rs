//! Item lowering: functions, structs, enums.

use super::block::lower_block;
use super::ty::lower_ty;
use super::{name_text, token_text, Def, DefKind, LowerCtx};
use crate::hir::*;
use crate::HirDiagnostic;
use axiom_parser::ast::{self, AstNode};

pub(super) fn lower_item(node: axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Item> {
    let kind = node.kind();
    if let Some(fn_def) = ast::FnDef::cast(node.clone()) {
        Some(Item::FnDef(lower_fn_def(&fn_def, ctx)))
    } else if let Some(struct_def) = ast::StructDef::cast(node.clone()) {
        Some(Item::StructDef(lower_struct_def(&struct_def, ctx)))
    } else if let Some(enum_def) = ast::EnumDef::cast(node) {
        Some(Item::EnumDef(lower_enum_def(&enum_def, ctx)))
    } else {
        ctx.diag(HirDiagnostic::NotYetSupported {
            feature: format!("{kind:?}"),
            span: ctx.span_of(&axiom_parser::SyntaxNode::new_root(
                axiom_parser::GreenNodeBuilder::new().finish(),
            )),
        });
        None
    }
}

fn lower_fn_def(f: &ast::FnDef, ctx: &mut LowerCtx) -> FnDef {
    let id = ctx.alloc_id();
    let fname = f.name().map(|n| name_text(&n)).unwrap_or_default();
    let visibility = if f.visibility().is_some() {
        Visibility::Public
    } else {
        Visibility::Private
    };
    let params = lower_params(f.param_list(), ctx);
    let return_type = f
        .ret_type()
        .and_then(|r| r.ty())
        .map(|ty_node| lower_ty(&ty_node, ctx));
    let body = f
        .body()
        .map(|b| lower_block(&b, ctx))
        .unwrap_or_else(|| Block {
            id: ctx.alloc_id(),
            stmts: Vec::new(),
            tail: None,
        });

    ctx.defs.push(Def {
        name: fname.clone(),
        def_id: id,
        kind: DefKind::Fn,
    });

    FnDef {
        id,
        name: fname,
        visibility,
        params,
        return_type,
        body,
    }
}

fn lower_params(param_list: Option<ast::ParamList>, ctx: &mut LowerCtx) -> Vec<Param> {
    let Some(pl) = param_list else {
        return Vec::new();
    };
    pl.params()
        .into_iter()
        .map(|param| {
            let id = ctx.alloc_id();
            let convention = param
                .convention_token()
                .map(|t| match t.text() {
                    "inout" => CallingConvention::Inout,
                    "sink" => CallingConvention::Sink,
                    _ => CallingConvention::Let,
                })
                .unwrap_or(CallingConvention::Let);
            let pname = token_text(param.name_token());
            let ty = param.ty().map(|ty_node| lower_ty(&ty_node, ctx));

            ctx.defs.push(Def {
                name: pname.clone(),
                def_id: id,
                kind: DefKind::Param,
            });

            Param {
                id,
                convention,
                name: pname,
                ty,
            }
        })
        .collect()
}

fn lower_struct_def(s: &ast::StructDef, ctx: &mut LowerCtx) -> StructDef {
    let id = ctx.alloc_id();
    let sname = s.name().map(|n| name_text(&n)).unwrap_or_default();
    let visibility = if s.visibility().is_some() {
        Visibility::Public
    } else {
        Visibility::Private
    };
    let fields = s
        .field_list()
        .map(|fl| fl.fields())
        .unwrap_or_default()
        .into_iter()
        .map(|f| {
            let fid = ctx.alloc_id();
            let fname = token_text(f.name_token());
            let fty = f
                .ty()
                .map(|ty_node| lower_ty(&ty_node, ctx))
                .unwrap_or(HirTy::Error);
            let fvis = if f.visibility().is_some() {
                Visibility::Public
            } else {
                Visibility::Private
            };
            ctx.defs.push(Def {
                name: fname.clone(),
                def_id: fid,
                kind: DefKind::Field,
            });
            FieldDef {
                id: fid,
                name: fname,
                ty: fty,
                visibility: fvis,
            }
        })
        .collect();

    ctx.defs.push(Def {
        name: sname.clone(),
        def_id: id,
        kind: DefKind::Struct,
    });

    StructDef {
        id,
        name: sname,
        visibility,
        fields,
    }
}

fn lower_enum_def(e: &ast::EnumDef, ctx: &mut LowerCtx) -> EnumDef {
    let id = ctx.alloc_id();
    let ename = e.name().map(|n| name_text(&n)).unwrap_or_default();
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
            let payload = v
                .payload()
                .map(|p| {
                    p.types()
                        .into_iter()
                        .map(|ty_node| lower_ty(&ty_node, ctx))
                        .collect()
                })
                .unwrap_or_default();
            ctx.defs.push(Def {
                name: vname.clone(),
                def_id: vid,
                kind: DefKind::Variant,
            });
            VariantDef {
                id: vid,
                name: vname,
                payload,
            }
        })
        .collect();

    ctx.defs.push(Def {
        name: ename.clone(),
        def_id: id,
        kind: DefKind::Enum,
    });

    EnumDef {
        id,
        name: ename,
        visibility,
        variants,
    }
}
