//! Pure helper functions for monomorphization: unification, substitution,
//! type-parameter detection, and name mangling.

use std::collections::HashMap;

use typecheck::types::{EnumTy, ErrorSetTy, FnTy, InstanceTy, StructTy, Ty, TypeParamId};

pub type Substitution = HashMap<TypeParamId, Ty>;

// ── Type-param detection ──────────────────────────────────────────────────────

/// Check if a list of types contains any `TypeParam`.
pub fn contains_type_param_tys(tys: &[Ty]) -> bool {
    tys.iter().any(contains_type_param)
}

/// Check if a type tree contains any `TypeParam` (recursive).
pub fn contains_type_param(ty: &Ty) -> bool {
    match ty {
        Ty::TypeParam(_) => true,
        Ty::Fn(f) => {
            f.params.iter().any(contains_type_param) || contains_type_param(&f.return_type)
        }
        Ty::Tuple(elems) => elems.iter().any(contains_type_param),
        Ty::Instance(i) => i.args.iter().any(contains_type_param),
        _ => false,
    }
}

// ── Unification ───────────────────────────────────────────────────────────────

/// Unify `actual` (concrete) with `expected` (may contain `TypeParam`),
/// recording `TypeParam → concrete` mappings in `subst`.
pub fn unify(actual: &Ty, expected: &Ty, subst: &mut Substitution) {
    match (actual, expected) {
        (_, Ty::TypeParam(tp)) => {
            subst.entry(tp.clone()).or_insert_with(|| actual.clone());
        }
        (Ty::Fn(a), Ty::Fn(e)) => {
            for (at, et) in a.params.iter().zip(e.params.iter()) {
                unify(at, et, subst);
            }
            unify(&a.return_type, &e.return_type, subst);
        }
        (Ty::Tuple(a), Ty::Tuple(e)) if a.len() == e.len() => {
            for (at, et) in a.iter().zip(e.iter()) {
                unify(at, et, subst);
            }
        }
        (Ty::Instance(a), Ty::Instance(e)) if a.name == e.name => {
            for (at, et) in a.args.iter().zip(e.args.iter()) {
                unify(at, et, subst);
            }
        }
        _ => {}
    }
}

// ── Substitution ──────────────────────────────────────────────────────────────

/// Replace every `Ty::TypeParam` in `ty` with the concrete type from `subst`.
pub fn substitute(ty: &Ty, subst: &Substitution) -> Ty {
    match ty {
        Ty::TypeParam(tp) => subst.get(tp).cloned().unwrap_or_else(|| ty.clone()),
        Ty::Fn(f) => Ty::Fn(FnTy {
            params: f.params.iter().map(|t| substitute(t, subst)).collect(),
            return_type: Box::new(substitute(&f.return_type, subst)),
        }),
        Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|t| substitute(t, subst)).collect()),
        Ty::Instance(i) => Ty::Instance(InstanceTy {
            name: i.name.clone(),
            def_id: i.def_id,
            args: i.args.iter().map(|t| substitute(t, subst)).collect(),
        }),
        Ty::HeapBuffer(inner) => Ty::HeapBuffer(Box::new(substitute(inner, subst))),
        Ty::Struct(_)
        | Ty::Enum(_)
        | Ty::ErrorSet(_)
        | Ty::Int
        | Ty::Float
        | Ty::Bool
        | Ty::String
        | Ty::Unit
        | Ty::Error => ty.clone(),
    }
}

// ── Name mangling ─────────────────────────────────────────────────────────────

/// Mangled name: `original__Type1_Type2` (e.g., `id__Int`, `pair__Int_String`).
pub fn mangle_name(original: &str, type_args: &[Ty]) -> String {
    let arg_names: Vec<String> = type_args.iter().map(type_arg_name).collect();
    format!("{original}__{}", arg_names.join("_"))
}

/// Join the short names of all type args with `_` (used as dedup key suffix).
pub fn type_args_suffix(type_args: &[Ty]) -> String {
    type_args
        .iter()
        .map(type_arg_name)
        .collect::<Vec<_>>()
        .join("_")
}

/// Short name for a type argument in a mangled name.
pub fn type_arg_name(ty: &Ty) -> String {
    match ty {
        Ty::Int => "Int".to_string(),
        Ty::Float => "Float".to_string(),
        Ty::Bool => "Bool".to_string(),
        Ty::String => "String".to_string(),
        Ty::Unit => "Unit".to_string(),
        Ty::Struct(StructTy { name, .. }) => name.clone(),
        Ty::Enum(EnumTy { name, .. }) => name.clone(),
        Ty::Fn(_) => "Fn".to_string(),
        Ty::Tuple(elems) => {
            let names: Vec<String> = elems.iter().map(type_arg_name).collect();
            format!("Tuple_{}", names.join("_"))
        }
        Ty::TypeParam(tp) => tp.name.clone(),
        Ty::Instance(InstanceTy { name, args, .. }) => {
            let arg_names: Vec<String> = args.iter().map(type_arg_name).collect();
            format!("{name}_{}", arg_names.join("_"))
        }
        Ty::HeapBuffer(inner) => format!("HeapBuffer_{}", type_arg_name(inner)),
        Ty::ErrorSet(ErrorSetTy { name, .. }) => name.clone(),
        Ty::Error => "Error".to_string(),
    }
}
