//! Item lowering: HIR Item → IR function.

use std::collections::HashMap;

use super::helpers::FnLowerCtx;
use crate::ir::{GenericOrigin, IrFunction, IrParam};
use axiom_hir::{FnDef, Item};
use axiom_typeck::mono::helpers::Substitution;
use axiom_typeck::{Ty, TypeParamId};

use super::LowerCtx;

/// Lower an HIR item. Only non-generic FnDefs produce IR functions.
/// Generic FnDefs are skipped — they are replaced by monomorphized instances
/// lowered in [`lower_mono_instances`].
/// EnumDefs are collected into `enum_variants` so the VM can distinguish
/// enum constructor calls from function calls.
pub(super) fn lower_item(item: &Item, ctx: &mut LowerCtx) {
    match item {
        Item::FnDef(f) => {
            // Skip generic FnDefs — they are lowered as monomorphized instances.
            if !f.type_params.is_empty() {
                return;
            }
            lower_fn_def(f, ctx, None, None);
        }
        Item::EnumDef(e) => {
            for v in &e.variants {
                ctx.enum_variants
                    .insert(v.name.clone(), (e.name.clone(), v.payload.len()));
            }
        }
        // ImplDef methods are registered with qualified names ("Type::method")
        // to avoid collisions when two impls define the same method name.
        Item::ImplDef(impl_def) => {
            let type_name = name_ref_text(&impl_def.type_name);
            for m in &impl_def.methods {
                if m.type_params.is_empty() {
                    lower_fn_def(m, ctx, None, Some(&type_name));
                }
            }
        }
        Item::StructDef(_) | Item::TraitDef(_) | Item::SubscriptDef(_) | Item::UseItem(_) => {}
    }
}

/// Lower all monomorphized function instances from the MonoResult.
pub(super) fn lower_mono_instances(ctx: &mut LowerCtx) {
    for inst in &ctx.mono.instances {
        let fndef = match find_fn_def(inst.original_id, &ctx.thir.hir.items) {
            Some(f) => f,
            None => continue,
        };
        let subst = build_subst(fndef, &inst.type_args);
        lower_fn_def(
            fndef,
            ctx,
            Some((&inst.name, &subst, &inst.param_types, &inst.return_type)),
            None,
        );
    }
}

/// Find the original FnDef by HirId, searching top-level items and ImplDef methods.
fn find_fn_def(id: axiom_hir::HirId, items: &[Item]) -> Option<&FnDef> {
    for item in items {
        match item {
            Item::FnDef(f) if f.id == id => return Some(f),
            Item::ImplDef(impl_def) => {
                for m in &impl_def.methods {
                    if m.id == id {
                        return Some(m);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Build a TypeParamId → concrete Ty substitution from a FnDef's type params
/// and the concrete type args.
fn build_subst(fndef: &FnDef, type_args: &[Ty]) -> Substitution {
    let mut subst = HashMap::new();
    for (i, tp) in fndef.type_params.iter().enumerate() {
        if let Some(concrete) = type_args.get(i) {
            subst.insert(
                TypeParamId {
                    name: tp.name.clone(),
                    index: i,
                    def_id: tp.id,
                },
                concrete.clone(),
            );
        }
    }
    subst
}

/// Extract the text from a NameRef (resolved or unresolved).
fn name_ref_text(nr: &axiom_hir::NameRef) -> String {
    match nr {
        axiom_hir::NameRef::Resolved(r) => r.text.clone(),
        axiom_hir::NameRef::Unresolved(u) => u.text.clone(),
    }
}

/// Lower a function definition. For monomorphized instances, `mono_info` carries
/// the mangled name, substitution, concrete param types, and return type.
/// For impl methods, `type_prefix` qualifies the name as "Type::method".
fn lower_fn_def(
    fndef: &FnDef,
    ctx: &mut LowerCtx,
    mono_info: Option<(&str, &Substitution, &[Ty], &Ty)>,
    type_prefix: Option<&str>,
) {
    let (name, subst, mono_param_tys, mono_ret_ty) = match &mono_info {
        Some((name, subst, param_tys, ret_ty)) => (
            name.to_string(),
            Some(*subst),
            Some(*param_tys),
            Some(*ret_ty),
        ),
        None => {
            let base = fndef.name.clone();
            let qualified = match type_prefix {
                Some(prefix) => format!("{prefix}::{base}"),
                None => base,
            };
            (qualified, None, None, None)
        }
    };

    let mut fn_ctx = FnLowerCtx::new(&ctx.thir.types, &ctx.mono_lookup, subst);

    // Allocate registers for parameters.
    let params: Vec<IrParam> = fndef
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let reg = fn_ctx.fresh_reg();
            let ty = mono_param_tys
                .and_then(|pts| pts.get(i).cloned())
                .or_else(|| ctx.thir.types.get(&p.id).cloned())
                .unwrap_or(Ty::Error);
            fn_ctx.bind(p.id, reg);
            IrParam {
                reg,
                name: p.name.clone(),
                ty,
            }
        })
        .collect();

    // Determine return type.
    let return_type = mono_ret_ty
        .cloned()
        .or_else(|| ctx.thir.types.get(&fndef.id).cloned())
        .unwrap_or(Ty::Unit);

    // Lower the body block.
    let entry_label = "entry".to_string();
    fn_ctx.start_block(entry_label);

    let tail_reg = super::stmt::lower_block_expr(&fndef.body, &mut fn_ctx);

    // Add implicit return carrying the tail expression's value.
    fn_ctx.ensure_return(Some(tail_reg));

    let generic_origin = mono_info
        .as_ref()
        .map(|(_name, _, type_args, _)| GenericOrigin {
            generic_name: fndef.name.clone(),
            concrete_args: (*type_args).to_vec(),
        });

    let func = IrFunction {
        name,
        type_params: Vec::new(), // Monomorphized instances have no type params.
        generic_origin,
        params,
        return_type,
        blocks: fn_ctx.func.blocks,
        next_reg: fn_ctx.func.next_reg,
    };

    ctx.functions.push(func);
}
