//! Unit tests for monomorphization helpers.

use axiom_hir::HirId;

use super::helpers::*;
use crate::types::{FnTy, StructTy, Ty, TypeParamId};

fn tp(name: &str, index: usize, def_id: usize) -> TypeParamId {
    TypeParamId {
        name: name.to_string(),
        index,
        def_id: HirId(def_id),
    }
}

#[test]
fn test_contains_type_param() {
    assert!(contains_type_param(&Ty::TypeParam(tp("T", 0, 0))));
    assert!(!contains_type_param(&Ty::Int));
    assert!(!contains_type_param(&Ty::Struct(StructTy {
        name: "Foo".to_string(),
        def_id: HirId(0),
    })));
}

#[test]
fn test_contains_type_param_in_fn() {
    let fnty = Ty::Fn(FnTy {
        params: vec![Ty::TypeParam(tp("T", 0, 0))],
        return_type: Box::new(Ty::Int),
    });
    assert!(contains_type_param(&fnty));
}

#[test]
fn test_contains_type_param_tuple() {
    let tup = Ty::Tuple(vec![Ty::Int, Ty::TypeParam(tp("T", 0, 0))]);
    assert!(contains_type_param(&tup));
    assert!(!contains_type_param(&Ty::Tuple(vec![Ty::Int, Ty::Float])));
}

#[test]
fn test_unify_concrete_with_typeparam() {
    let t = tp("T", 0, 0);
    let mut subst = Substitution::new();
    unify(&Ty::Int, &Ty::TypeParam(t.clone()), &mut subst);
    assert_eq!(subst.get(&t), Some(&Ty::Int));
}

#[test]
fn test_unify_fn_types() {
    let t = tp("T", 0, 0);
    let actual = Ty::Fn(FnTy {
        params: vec![Ty::Int],
        return_type: Box::new(Ty::Int),
    });
    let expected = Ty::Fn(FnTy {
        params: vec![Ty::TypeParam(t.clone())],
        return_type: Box::new(Ty::TypeParam(t.clone())),
    });
    let mut subst = Substitution::new();
    unify(&actual, &expected, &mut subst);
    assert_eq!(subst.get(&t), Some(&Ty::Int));
}

#[test]
fn test_unify_idempotent() {
    let t = tp("T", 0, 0);
    let mut subst = Substitution::new();
    unify(&Ty::String, &Ty::TypeParam(t.clone()), &mut subst);
    unify(&Ty::Int, &Ty::TypeParam(t.clone()), &mut subst);
    // First binding wins.
    assert_eq!(subst.get(&t), Some(&Ty::String));
}

#[test]
fn test_substitute_typeparam() {
    let t = tp("T", 0, 0);
    let mut subst = Substitution::new();
    subst.insert(t.clone(), Ty::Int);
    assert_eq!(substitute(&Ty::TypeParam(t), &subst), Ty::Int);
    assert_eq!(substitute(&Ty::Float, &subst), Ty::Float);
}

#[test]
fn test_substitute_fn() {
    let t = tp("T", 0, 0);
    let mut subst = Substitution::new();
    subst.insert(t.clone(), Ty::String);
    let fnty = Ty::Fn(FnTy {
        params: vec![Ty::TypeParam(t.clone())],
        return_type: Box::new(Ty::TypeParam(t)),
    });
    let result = substitute(&fnty, &subst);
    let expected = Ty::Fn(FnTy {
        params: vec![Ty::String],
        return_type: Box::new(Ty::String),
    });
    assert_eq!(result, expected);
}

#[test]
fn test_substitute_tuple() {
    let t = tp("T", 0, 0);
    let mut subst = Substitution::new();
    subst.insert(t.clone(), Ty::Bool);
    let tup = Ty::Tuple(vec![Ty::TypeParam(t), Ty::Int]);
    assert_eq!(substitute(&tup, &subst), Ty::Tuple(vec![Ty::Bool, Ty::Int]));
}

#[test]
fn test_mangle_name_single() {
    assert_eq!(mangle_name("id", &[Ty::Int]), "id__Int");
    assert_eq!(mangle_name("f", &[Ty::Bool]), "f__Bool");
}

#[test]
fn test_mangle_name_multiple() {
    assert_eq!(
        mangle_name("pair", &[Ty::Int, Ty::String]),
        "pair__Int_String"
    );
}

#[test]
fn test_mangle_name_struct() {
    let s = Ty::Struct(StructTy {
        name: "Point".to_string(),
        def_id: HirId(0),
    });
    assert_eq!(mangle_name("make", &[s]), "make__Point");
}

#[test]
fn test_type_arg_name_primitives() {
    assert_eq!(type_arg_name(&Ty::Int), "Int");
    assert_eq!(type_arg_name(&Ty::Float), "Float");
    assert_eq!(type_arg_name(&Ty::Bool), "Bool");
    assert_eq!(type_arg_name(&Ty::String), "String");
    assert_eq!(type_arg_name(&Ty::Unit), "Unit");
}
