//! Item lowering: HIR Item → IR function.

use super::helpers::FnLowerCtx;
use crate::ir::{IrFunction, IrParam};
use axiom_hir::{FnDef, Item};
use axiom_typeck::Ty;

use super::LowerCtx;

/// Lower an HIR item. Only FnDefs produce IR functions in v0.
/// EnumDefs are collected into `enum_variants` so the VM can distinguish
/// enum constructor calls from function calls.
pub(super) fn lower_item(item: &Item, ctx: &mut LowerCtx) {
    match item {
        Item::FnDef(f) => lower_fn_def(f, ctx),
        Item::EnumDef(e) => {
            for v in &e.variants {
                ctx.enum_variants
                    .insert(v.name.clone(), (e.name.clone(), v.payload.len()));
            }
        }
        Item::StructDef(_) | Item::TraitDef(_) | Item::ImplDef(_) | Item::SubscriptDef(_) => {}
    }
}

fn lower_fn_def(fndef: &FnDef, ctx: &mut LowerCtx) {
    let mut fn_ctx = FnLowerCtx::new(&ctx.thir.types);

    // Allocate registers for parameters.
    let params: Vec<IrParam> = fndef
        .params
        .iter()
        .map(|p| {
            let reg = fn_ctx.fresh_reg();
            let ty = ctx.thir.types.get(&p.id).cloned().unwrap_or(Ty::Error);
            fn_ctx.bind(p.id, reg);
            IrParam {
                reg,
                name: p.name.clone(),
                ty,
            }
        })
        .collect();

    // Determine return type.
    let return_type = ctx.thir.types.get(&fndef.id).cloned().unwrap_or(Ty::Unit);

    // Lower the body block.
    let entry_label = "entry".to_string();
    fn_ctx.start_block(entry_label);

    let tail_reg = super::stmt::lower_block_expr(&fndef.body, &mut fn_ctx);

    // Add implicit return carrying the tail expression's value.
    fn_ctx.ensure_return(Some(tail_reg));

    let func = IrFunction {
        name: fndef.name.clone(),
        type_params: fndef.type_params.iter().map(|tp| tp.name.clone()).collect(),
        generic_origin: None,
        params,
        return_type,
        blocks: fn_ctx.func.blocks,
        next_reg: fn_ctx.func.next_reg,
    };

    ctx.functions.push(func);
}
