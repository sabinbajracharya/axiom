//! Item lowering: HIR Item → IR function.

use std::collections::HashMap;

use super::helpers::FnLowerCtx;
use crate::ir::{GenericOrigin, IrFunction, IrParam};
use resolver::{FnDef, Item};
use typecheck::mono::helpers::Substitution;
use typecheck::{Ty, TypeParamId};

use super::LowerCtx;

/// Lower an HIR item. Only non-generic FnDefs produce IR functions.
/// Generic FnDefs are skipped — they are replaced by monomorphized instances
/// lowered in [`lower_mono_instances`].
/// EnumDefs are collected into `enum_variants` so the VM can distinguish
/// enum constructor calls from function calls.
pub(super) fn lower_item(item: &Item, ctx: &mut LowerCtx) {
    match item {
        Item::FnDef(f) => {
            // Skip `@intrinsic` functions — the compiler emits the corresponding
            // IR instruction inline at each call site; no function body to lower.
            if f.intrinsic_tag.is_some() {
                return;
            }
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
                if m.intrinsic_tag.is_some() {
                    continue; // Skip @intrinsic — compiler emits inline.
                }
                if m.type_params.is_empty() {
                    lower_fn_def(m, ctx, None, Some(&type_name));
                }
            }
            // A subscript lowers to the function `Type::subscript(self, index)`;
            // `base[index]` dispatches to it (see `lower_index`). Like a method
            // on a generic type it is lowered once and works for any element
            // type (the VM is dynamically typed).
            for sub in &impl_def.subscripts {
                lower_subscript(sub, ctx, &type_name);
            }
            // A trait default method the impl does not override is lowered as a
            // copy named `Type::method`, so `value.method()` dispatches to it by
            // the receiver's runtime type. The body is `Self`-generic (calls on
            // `self` stay unqualified and resolve at runtime), so the copies are
            // identical apart from their name.
            lower_inherited_defaults(impl_def, ctx, &type_name);
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
fn find_fn_def(id: resolver::HirId, items: &[Item]) -> Option<&FnDef> {
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
fn name_ref_text(nr: &resolver::NameRef) -> String {
    match nr {
        resolver::NameRef::Resolved(r) => r.text.clone(),
        resolver::NameRef::Unresolved(u) => u.text.clone(),
    }
}

/// Build the qualified IR name for a non-monomorphized function.
/// Uses type_prefix (for impl methods) or module_path (for cross-module fns).
fn qualified_name(fndef: &FnDef, type_prefix: Option<&str>) -> String {
    let base = fndef.name.clone();
    match type_prefix {
        Some(prefix) => format!("{prefix}::{base}"),
        None if !fndef.module_path.is_empty() => format!("{}::{}", fndef.module_path, base),
        None => base,
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
        None => (qualified_name(fndef, type_prefix), None, None, None),
    };

    let mut fn_ctx = FnLowerCtx::new(
        &ctx.thir.types,
        &ctx.mono_lookup,
        subst,
        &ctx.thir.hir.items,
    );

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
                convention: p.convention,
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
        is_extern: fndef.extern_abi.is_some(),
    };

    ctx.functions.push(func);
}

/// Lower a subscript operator to the IR function `Type::subscript(self, index…)`.
/// Mirrors the non-generic method path of [`lower_fn_def`]: `base[index]`
/// lowers to a `MethodCall` on this function (see `lower_index`).
fn lower_subscript(sub: &resolver::SubscriptDef, ctx: &mut LowerCtx, type_prefix: &str) {
    let mut fn_ctx = FnLowerCtx::new(&ctx.thir.types, &ctx.mono_lookup, None, &ctx.thir.hir.items);

    let params: Vec<IrParam> = sub
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
                convention: p.convention,
            }
        })
        .collect();

    let return_type = ctx.thir.types.get(&sub.id).cloned().unwrap_or(Ty::Unit);

    fn_ctx.start_block("entry".to_string());
    let tail_reg = super::stmt::lower_block_expr(&sub.body, &mut fn_ctx);
    fn_ctx.ensure_return(Some(tail_reg));

    // A read subscript lowers to `Type::subscript`; a setter (no return type)
    // to `Type::subscript_set`, so `base[i]` and `base[i] = v` dispatch to
    // distinct functions (`docs/mutable-subscript-design.md` §4.2).
    let name = if sub.is_setter {
        resolver::lang::subscript_set_fn(type_prefix)
    } else {
        resolver::lang::subscript_fn(type_prefix)
    };

    ctx.functions.push(IrFunction {
        name,
        type_params: Vec::new(),
        generic_origin: None,
        params,
        return_type,
        blocks: fn_ctx.func.blocks,
        next_reg: fn_ctx.func.next_reg,
        is_extern: false,
    });
}

/// Lower each trait default method that `impl_def` inherits (does not override)
/// as a `Type::method` IR function. Called from `lower_item` for trait impls.
fn lower_inherited_defaults(impl_def: &resolver::ImplDef, ctx: &mut LowerCtx, type_prefix: &str) {
    let Some(trait_nr) = &impl_def.trait_name else {
        return;
    };
    let trait_name = name_ref_text(trait_nr);
    let overridden: Vec<String> = impl_def.methods.iter().map(|m| m.name.clone()).collect();
    // Clone the inherited defaults out first so the `ctx.thir` borrow ends
    // before lowering, which needs `&mut ctx`.
    let defaults: Vec<resolver::TraitMethod> = ctx
        .thir
        .hir
        .items
        .iter()
        .find_map(|item| match item {
            Item::TraitDef(t) if t.name == trait_name => Some(t),
            _ => None,
        })
        .map(|t| {
            t.methods
                .iter()
                .filter(|m| m.body.is_some() && !overridden.iter().any(|o| o == &m.name))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    for m in &defaults {
        lower_trait_default(m, ctx, type_prefix);
    }
}

/// Lower a trait default method body as the IR function `Type::method`. The
/// body is `Self`-generic: calls on `self` stay unqualified and resolve to the
/// receiver's concrete impl at runtime (see `resolve_method_target` in the VM).
fn lower_trait_default(m: &resolver::TraitMethod, ctx: &mut LowerCtx, type_prefix: &str) {
    let Some(body) = &m.body else {
        return;
    };
    let mut fn_ctx = FnLowerCtx::new(&ctx.thir.types, &ctx.mono_lookup, None, &ctx.thir.hir.items);

    let params: Vec<IrParam> = m
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
                convention: p.convention,
            }
        })
        .collect();

    let return_type = ctx.thir.types.get(&m.id).cloned().unwrap_or(Ty::Unit);

    fn_ctx.start_block("entry".to_string());
    let tail_reg = super::stmt::lower_block_expr(body, &mut fn_ctx);
    fn_ctx.ensure_return(Some(tail_reg));

    ctx.functions.push(IrFunction {
        name: format!("{type_prefix}::{}", m.name),
        type_params: Vec::new(),
        generic_origin: None,
        params,
        return_type,
        blocks: fn_ctx.func.blocks,
        next_reg: fn_ctx.func.next_reg,
        is_extern: false,
    });
}
