//! Item-level name resolution: functions, structs, enums, traits, impls.

use super::{resolve_name_ref, Scope};
use crate::hir_types::*;
use crate::lowering::DefKind;
use crate::HirDiagnostic;
use std::collections::HashMap;

pub(super) fn resolve_item_names(
    item: &mut Item,
    top_level: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    match item {
        Item::FnDef(f) => {
            let mut scope = Scope::new_child(top_level);
            for tp in &f.type_params {
                scope.define(tp.name.clone(), tp.id, DefKind::TypeParam);
            }
            for param in &mut f.params {
                if let Some(ty) = &mut param.ty {
                    resolve_ty_names(ty, &scope.bindings);
                }
                scope.define(param.name.clone(), param.id, DefKind::Param);
            }
            if let Some(ret) = &mut f.return_type {
                resolve_ty_names(ret, &scope.bindings);
            }
            super::body::resolve_block_names(&mut f.body, &scope, diagnostics);
        }
        Item::StructDef(s) => {
            let mut scope = Scope::new_child(top_level);
            for tp in &s.type_params {
                scope.define(tp.name.clone(), tp.id, DefKind::TypeParam);
            }
            for field in &mut s.fields {
                resolve_ty_names(&mut field.ty, &scope.bindings);
            }
        }
        Item::EnumDef(e) => {
            let mut scope = Scope::new_child(top_level);
            for tp in &e.type_params {
                scope.define(tp.name.clone(), tp.id, DefKind::TypeParam);
            }
            for variant in &mut e.variants {
                for payload_ty in &mut variant.payload {
                    resolve_ty_names(payload_ty, &scope.bindings);
                }
            }
        }
        Item::TraitDef(t) => resolve_trait_def(t, top_level, diagnostics),
        Item::ImplDef(impl_def) => resolve_impl_def(impl_def, top_level, diagnostics),
        Item::SubscriptDef(s) => {
            let mut scope = Scope::new_child(top_level);
            for param in &mut s.params {
                if let Some(ty) = &mut param.ty {
                    resolve_ty_names(ty, &scope.bindings);
                }
                scope.define(param.name.clone(), param.id, DefKind::Param);
            }
            if let Some(ret) = &mut s.return_type {
                resolve_ty_names(ret, &scope.bindings);
            }
            super::body::resolve_block_names(&mut s.body, &scope, diagnostics);
        }
        Item::UseItem(_) => {
            // Use items are processed separately during import resolution.
        }
        Item::ErrorSetDef(_) => {
            // Error sets have no type params and no payload types — nothing to resolve.
        }
    }
}

fn resolve_trait_def(
    t: &mut TraitDef,
    top_level: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    let mut scope = Scope::new_child(top_level);
    for tp in &t.type_params {
        scope.define(tp.name.clone(), tp.id, DefKind::TypeParam);
    }
    for method in &mut t.methods {
        resolve_method_sig(
            &mut method.params,
            &mut method.return_type,
            &scope,
            method.body.as_mut(),
            diagnostics,
        );
    }
}

fn resolve_impl_def(
    impl_def: &mut ImplDef,
    top_level: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    if let Some(trait_nr) = &mut impl_def.trait_name {
        resolve_name_ref(trait_nr, top_level, diagnostics);
    }
    resolve_name_ref(&mut impl_def.type_name, top_level, diagnostics);
    let mut scope = Scope::new_child(top_level);
    for tp in &impl_def.type_params {
        scope.define(tp.name.clone(), tp.id, DefKind::TypeParam);
    }
    for method in &mut impl_def.methods {
        resolve_method_sig(
            &mut method.params,
            &mut method.return_type,
            &scope,
            Some(&mut method.body),
            diagnostics,
        );
    }
    // Subscripts resolve like methods (their synthesized `self` + index params
    // are in `params`), so the body's `self`/index references bind.
    for sub in &mut impl_def.subscripts {
        resolve_method_sig(
            &mut sub.params,
            &mut sub.return_type,
            &scope,
            Some(&mut sub.body),
            diagnostics,
        );
    }
}

/// Resolve param types, register param names, resolve return type,
/// and optionally resolve a body with the param scope.
fn resolve_method_sig(
    params: &mut [Param],
    return_type: &mut Option<HirTy>,
    scope: &Scope,
    body: Option<&mut Block>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    let mut mscope = Scope::new_child(&scope.bindings);
    for param in params.iter_mut() {
        if let Some(ty) = &mut param.ty {
            resolve_ty_names(ty, &mscope.bindings);
        }
        mscope.define(param.name.clone(), param.id, DefKind::Param);
    }
    if let Some(ret) = return_type {
        resolve_ty_names(ret, &mscope.bindings);
    }
    if let Some(body) = body {
        super::body::resolve_block_names(body, &mscope, diagnostics);
    }
}

/// Resolve type parameter names within a `HirTy`.
pub(super) fn resolve_ty_names(ty: &mut HirTy, bindings: &HashMap<String, (DefId, DefKind)>) {
    match ty {
        HirTy::Named(nr) => {
            let text = match nr {
                NameRef::Resolved(_) => return,
                NameRef::Unresolved(u) => u.text.clone(),
            };
            if let Some((def_id, kind)) = bindings.get(&text) {
                if *kind == DefKind::TypeParam {
                    *ty = HirTy::TypeParam(HirTypeParam {
                        id: *def_id,
                        name: text,
                        bounds: Vec::new(),
                    });
                } else {
                    *nr = NameRef::resolved(*def_id, &text);
                }
            }
        }
        HirTy::Instance(inst) => {
            let text = match &inst.name {
                NameRef::Resolved(_) => String::new(),
                NameRef::Unresolved(u) => u.text.clone(),
            };
            if !text.is_empty() {
                if let Some((def_id, _)) = bindings.get(&text) {
                    inst.name = NameRef::resolved(*def_id, &text);
                }
            }
            for arg in &mut inst.args {
                resolve_ty_names(arg, bindings);
            }
        }
        HirTy::Tuple(elems) => {
            for elem in elems {
                resolve_ty_names(elem, bindings);
            }
        }
        HirTy::Fn(f) => {
            for param in &mut f.params {
                resolve_ty_names(param, bindings);
            }
            resolve_ty_names(&mut f.return_type, bindings);
        }
        HirTy::Slice(elem) => {
            resolve_ty_names(elem, bindings);
        }
        HirTy::TypeParam(_) | HirTy::Unit | HirTy::Error => {}
    }
}
