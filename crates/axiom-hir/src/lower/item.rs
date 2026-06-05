//! Item lowering: functions, structs, enums.

use super::block::lower_block;
use super::ty::lower_ty;
use super::{name_text, path_last_segment, token_text, Def, DefKind, LowerCtx};
use crate::hir::*;
use crate::HirDiagnostic;
use axiom_parser::ast::{self, AstNode};

pub(super) fn lower_item(node: axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> Option<Item> {
    let kind = node.kind();
    if let Some(fn_def) = ast::FnDef::cast(node.clone()) {
        Some(Item::FnDef(lower_fn_def(&fn_def, ctx)))
    } else if let Some(struct_def) = ast::StructDef::cast(node.clone()) {
        Some(Item::StructDef(lower_struct_def(&struct_def, ctx)))
    } else if let Some(enum_def) = ast::EnumDef::cast(node.clone()) {
        Some(Item::EnumDef(lower_enum_def(&enum_def, ctx)))
    } else if let Some(trait_def) = ast::TraitDef::cast(node.clone()) {
        Some(Item::TraitDef(lower_trait_def(&trait_def, ctx)))
    } else if let Some(impl_block) = ast::ImplBlock::cast(node.clone()) {
        Some(Item::ImplDef(lower_impl_block(&impl_block, ctx)))
    } else if let Some(use_decl) = ast::UseDecl::cast(node) {
        Some(Item::UseItem(lower_use_decl(&use_decl, ctx)))
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
    let result = lower_fn_inner(f, ctx);
    ctx.defs.push(Def {
        name: result.name.clone(),
        def_id: result.id,
        kind: DefKind::Fn,
        visibility: result.visibility,
        span: ctx.span_of(f.syntax()),
    });
    result
}

/// Lower a function without registering it as a top-level definition.
/// Used for impl methods (registered via the impl block, not individually).
fn lower_fn_no_register(f: &ast::FnDef, ctx: &mut LowerCtx) -> FnDef {
    lower_fn_inner(f, ctx)
}

fn lower_fn_inner(f: &ast::FnDef, ctx: &mut LowerCtx) -> FnDef {
    let id = ctx.alloc_id();
    let fname = f.name().map(|n| name_text(&n)).unwrap_or_default();
    let visibility = if f.visibility().is_some() {
        Visibility::Public
    } else {
        Visibility::Private
    };
    let type_params = lower_generic_params(f.generic_param_list(), ctx);
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

    FnDef {
        id,
        name: fname,
        visibility,
        type_params,
        params,
        return_type,
        body,
    }
}

fn lower_params(param_list: Option<ast::ParamList>, ctx: &mut LowerCtx) -> Vec<Param> {
    let Some(pl) = param_list else {
        return Vec::new();
    };
    let mut result = Vec::new();
    // Lower the `self` receiver first, if present.
    if let Some(sp) = pl.self_param() {
        let id = ctx.alloc_id();
        let convention = sp
            .convention_token()
            .map(|t| match t.text() {
                "inout" => CallingConvention::Inout,
                "sink" => CallingConvention::Sink,
                _ => CallingConvention::Let,
            })
            .unwrap_or(CallingConvention::Let);
        ctx.defs.push(Def {
            name: "self".to_string(),
            def_id: id,
            kind: DefKind::Param,
            visibility: Visibility::Private,
            span: ctx.span_of(sp.syntax()),
        });
        result.push(Param {
            id,
            convention,
            name: "self".to_string(),
            ty: None,
        });
    }
    result.extend(pl.params().into_iter().map(|param| {
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
            visibility: Visibility::Private,
            span: ctx.span_of(param.syntax()),
        });

        Param {
            id,
            convention,
            name: pname,
            ty,
        }
    }));
    result
}

/// Lower `<T: Ord, U>` into `Vec<HirTypeParam>`, registering each param in `ctx.defs`.
fn lower_generic_params(
    params: Option<ast::GenericParamList>,
    ctx: &mut LowerCtx,
) -> Vec<HirTypeParam> {
    let Some(gp) = params else {
        return Vec::new();
    };
    gp.params()
        .into_iter()
        .map(|p| {
            let id = ctx.alloc_id();
            let pname = token_text(p.name_token());
            let bounds = p
                .bounds()
                .map(|b| {
                    b.types()
                        .into_iter()
                        .map(|ty_node| {
                            // Trait bounds are type nodes (PathType), not expr nodes.
                            let name = ast::PathType::cast(ty_node)
                                .and_then(|pt| pt.path())
                                .map(|p| NameRef::unresolved(path_last_segment(Some(p))))
                                .unwrap_or_else(|| NameRef::unresolved(""));
                            HirTraitBound { name }
                        })
                        .collect()
                })
                .unwrap_or_default();
            ctx.defs.push(Def {
                name: pname.clone(),
                def_id: id,
                kind: DefKind::TypeParam,
                visibility: Visibility::Private,
                span: ctx.span_of(p.syntax()),
            });
            HirTypeParam {
                id,
                name: pname,
                bounds,
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
    let type_params = lower_generic_params(s.generic_param_list(), ctx);
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
                visibility: fvis,
                span: ctx.span_of(f.syntax()),
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
        visibility,
        span: ctx.span_of(s.syntax()),
    });

    StructDef {
        id,
        name: sname,
        visibility,
        type_params,
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
    let type_params = lower_generic_params(e.generic_param_list(), ctx);
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
                visibility: Visibility::Private,
                span: ctx.span_of(v.syntax()),
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
        visibility,
        span: ctx.span_of(e.syntax()),
    });

    EnumDef {
        id,
        name: ename,
        visibility,
        type_params,
        variants,
    }
}

fn lower_trait_def(t: &ast::TraitDef, ctx: &mut LowerCtx) -> TraitDef {
    let id = ctx.alloc_id();
    let tname = t.name().map(|n| name_text(&n)).unwrap_or_default();
    let visibility = if t.visibility().is_some() {
        Visibility::Public
    } else {
        Visibility::Private
    };
    let type_params = lower_generic_params(t.generic_param_list(), ctx);
    let methods = t
        .item_list()
        .map(|il| il.methods())
        .unwrap_or_default()
        .into_iter()
        .map(|m| lower_trait_method(&m, ctx))
        .collect();

    ctx.defs.push(Def {
        name: tname.clone(),
        def_id: id,
        kind: DefKind::Trait,
        visibility,
        span: ctx.span_of(t.syntax()),
    });

    TraitDef {
        id,
        name: tname,
        visibility,
        type_params,
        methods,
    }
}

fn lower_trait_method(m: &ast::FnDef, ctx: &mut LowerCtx) -> TraitMethod {
    let id = ctx.alloc_id();
    let mname = m.name().map(|n| name_text(&n)).unwrap_or_default();
    let params = lower_params(m.param_list(), ctx);
    let return_type = m
        .ret_type()
        .and_then(|r| r.ty())
        .map(|ty_node| lower_ty(&ty_node, ctx));
    let body = m.body().map(|b| lower_block(&b, ctx));

    TraitMethod {
        id,
        name: mname,
        params,
        return_type,
        body,
    }
}

fn lower_impl_block(i: &ast::ImplBlock, ctx: &mut LowerCtx) -> ImplDef {
    let id = ctx.alloc_id();
    let type_params = lower_generic_params(i.generic_param_list(), ctx);

    // For `impl Trait for Type`, types() returns [Trait, Type].
    // For `impl Type`, types() returns [Type].
    let types = i.types();
    let (trait_name, type_name) = if types.len() >= 2 {
        let trait_nr = path_from_ast_type(&types[0], ctx);
        let type_nr = path_from_ast_type(&types[1], ctx);
        (Some(trait_nr), type_nr)
    } else if let Some(first) = types.first() {
        (None, path_from_ast_type(first, ctx))
    } else {
        (None, NameRef::unresolved(""))
    };

    let assoc = i.assoc_item_list();
    let methods = assoc
        .as_ref()
        .map(|il| il.methods())
        .unwrap_or_default()
        .into_iter()
        .map(|m| lower_fn_no_register(&m, ctx))
        .collect();
    let subscripts = assoc
        .map(|il| il.subscripts())
        .unwrap_or_default()
        .into_iter()
        .map(|s| lower_subscript_def(&s, ctx))
        .collect();

    ImplDef {
        id,
        trait_name,
        type_name,
        type_params,
        methods,
        subscripts,
    }
}

fn lower_subscript_def(s: &ast::SubscriptDef, ctx: &mut LowerCtx) -> SubscriptDef {
    let id = ctx.alloc_id();
    let params = lower_params(s.param_list(), ctx);
    let return_type = s.ret_type().and_then(|r| r.ty().map(|t| lower_ty(&t, ctx)));
    let body = s
        .body()
        .map(|b| lower_block(&b, ctx))
        .unwrap_or_else(|| Block {
            id: ctx.alloc_id(),
            stmts: vec![],
            tail: None,
        });
    SubscriptDef {
        id,
        params,
        return_type,
        body,
    }
}

/// Lower an AST type node to a `NameRef` (unresolved — resolved later by name resolution).
fn path_from_ast_type(ty_node: &ast::PathType, _ctx: &mut LowerCtx) -> NameRef {
    ty_node
        .path()
        .map(|p| NameRef::unresolved(path_last_segment(Some(p))))
        .unwrap_or_else(|| NameRef::unresolved(""))
}

/// Lower a `use` declaration into a `UseItem`.
fn lower_use_decl(decl: &ast::UseDecl, ctx: &mut LowerCtx) -> UseItem {
    let id = ctx.alloc_id();
    let visibility = if decl.visibility().is_some() {
        Visibility::Public
    } else {
        Visibility::Private
    };
    let tree = decl
        .use_tree()
        .map(|t| lower_use_tree(&t))
        .unwrap_or_else(|| UseTree {
            path: Vec::new(),
            kind: UseTreeKind::Single { rename: None },
        });
    UseItem {
        id,
        visibility,
        tree,
    }
}

/// Lower a `UseTree` CST node into a HIR `UseTree`.
fn lower_use_tree(node: &ast::UseTree) -> UseTree {
    use axiom_parser::{SyntaxElement, SyntaxKind};

    // Extract path segments from direct child PathSegment nodes.
    let path: Vec<String> = node
        .syntax()
        .children()
        .into_iter()
        .filter_map(|elem| match elem {
            SyntaxElement::Node(n) if n.kind() == SyntaxKind::PathSegment => {
                // PathSegment contains an Ident (or keyword) token child.
                n.children().into_iter().find_map(|child| match child {
                    SyntaxElement::Token(t)
                        if matches!(
                            t.kind(),
                            SyntaxKind::Ident
                                | SyntaxKind::KwSelf
                                | SyntaxKind::KwSelfType
                                | SyntaxKind::KwSuper
                                | SyntaxKind::KwCrate
                        ) =>
                    {
                        Some(t.text().to_string())
                    }
                    _ => None,
                })
            }
            _ => None,
        })
        .collect();

    // Check for a group `{ ... }`.
    if let Some(group) = node.group() {
        let trees = group.trees().iter().map(lower_use_tree).collect();
        return UseTree {
            path,
            kind: UseTreeKind::Group(trees),
        };
    }

    // Check for a glob `*`.
    let is_glob = node
        .syntax()
        .children()
        .into_iter()
        .any(|elem| matches!(elem, SyntaxElement::Token(t) if t.kind() == SyntaxKind::Star));
    if is_glob {
        return UseTree {
            path,
            kind: UseTreeKind::Glob,
        };
    }

    // Single import, possibly renamed.
    let rename = node
        .rename()
        .and_then(|r| r.name_token())
        .map(|t| t.text().to_string());
    UseTree {
        path,
        kind: UseTreeKind::Single { rename },
    }
}
