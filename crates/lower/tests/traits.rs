//! Trait and impl HIR lowering + name resolution tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lower::{serialize, HirTy, Item, NameRef};
use parser::ast::{AstNode, SourceFile};
use parser::parse;

fn lower_source(source: &str) -> lower::Hir {
    let result = parse(source);
    let root = SourceFile::cast(result.tree).unwrap();
    resolver::lower(&root, source, None)
}

// ── Trait declarations ───────────────────────────────────────────────────────

#[test]
fn test_trait_decl_required_method() {
    let hir = lower_source("trait Shape { fn area(let self) -> Float; }");
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    match &hir.items[0] {
        Item::TraitDef(t) => {
            assert_eq!(t.name, "Shape");
            assert_eq!(t.methods.len(), 1);
            assert_eq!(t.methods[0].name, "area");
            assert!(t.methods[0].body.is_none(), "required method has no body");
        }
        _ => panic!("expected TraitDef"),
    }
}

#[test]
fn test_trait_decl_default_method() {
    let hir = lower_source("trait Shape { fn name(let self) -> String { \"shape\" } }");
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    match &hir.items[0] {
        Item::TraitDef(t) => {
            assert_eq!(t.name, "Shape");
            assert_eq!(t.methods.len(), 1);
            assert_eq!(t.methods[0].name, "name");
            assert!(t.methods[0].body.is_some(), "default method has body");
        }
        _ => panic!("expected TraitDef"),
    }
}

#[test]
fn test_trait_decl_mixed_methods() {
    let src =
        "trait Shape { fn area(let self) -> Float; fn name(let self) -> String { \"shape\" } }";
    let hir = lower_source(src);
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    match &hir.items[0] {
        Item::TraitDef(t) => {
            assert_eq!(t.methods.len(), 2);
            assert!(t.methods[0].body.is_none());
            assert!(t.methods[1].body.is_some());
        }
        _ => panic!("expected TraitDef"),
    }
}

#[test]
fn test_trait_with_type_params() {
    let hir = lower_source("trait Container<T> { fn get(let self) -> T; }");
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    match &hir.items[0] {
        Item::TraitDef(t) => {
            assert_eq!(t.type_params.len(), 1);
            assert_eq!(t.type_params[0].name, "T");
            // Return type T should be resolved to TypeParam.
            match &t.methods[0].return_type {
                Some(HirTy::TypeParam(tp)) => assert_eq!(tp.name, "T"),
                other => panic!("expected TypeParam for return type, got: {:?}", other),
            }
        }
        _ => panic!("expected TraitDef"),
    }
}

// ── Impl blocks ──────────────────────────────────────────────────────────────

#[test]
fn test_impl_block_basic() {
    let src = "struct Circle { r: Float }\ntrait Shape { fn area(let self) -> Float; }\nimpl Shape for Circle { fn area(let self) -> Float { 3.14 } }";
    let hir = lower_source(src);
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    assert_eq!(hir.items.len(), 3);
    match (&hir.items[1], &hir.items[2]) {
        (Item::TraitDef(_), Item::ImplDef(i)) => {
            match &i.trait_name {
                Some(NameRef::Resolved(r)) => assert_eq!(r.text, "Shape"),
                other => panic!("expected resolved trait name, got: {:?}", other),
            }
            match &i.type_name {
                NameRef::Resolved(r) => assert_eq!(r.text, "Circle"),
                other => panic!("expected resolved type name, got: {:?}", other),
            }
            assert_eq!(i.methods.len(), 1);
            assert_eq!(i.methods[0].name, "area");
        }
        _ => panic!("expected TraitDef + ImplDef"),
    }
}

#[test]
fn test_impl_block_without_trait() {
    let src = "struct Circle { r: Float }\nimpl Circle { fn radius(let self) -> Float { 1.0 } }";
    let hir = lower_source(src);
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    assert_eq!(hir.items.len(), 2);
    match &hir.items[1] {
        Item::ImplDef(i) => {
            assert!(i.trait_name.is_none());
            match &i.type_name {
                NameRef::Resolved(r) => assert_eq!(r.text, "Circle"),
                other => panic!("expected resolved type name, got: {:?}", other),
            }
        }
        _ => panic!("expected ImplDef"),
    }
}

// ── Serialization ─────────────────────────────────────────────────────────────

#[test]
fn test_trait_serialize() {
    let src = "trait Shape { fn area(let self) -> Float; }";
    let hir = lower_source(src);
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    let dump = serialize(&hir);
    assert!(dump.contains("TraitDef("), "dump: {}", dump);
    assert!(dump.contains("name=Shape"), "dump: {}", dump);
    assert!(dump.contains("Method("), "dump: {}", dump);
}

#[test]
fn test_impl_serialize() {
    let src = "struct Circle { r: Float }\ntrait Shape { fn area(let self) -> Float; }\nimpl Shape for Circle { fn area(let self) -> Float { 3.14 } }";
    let hir = lower_source(src);
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    let dump = serialize(&hir);
    assert!(dump.contains("ImplDef("), "dump: {}", dump);
    assert!(dump.contains("Shape→"), "dump: {}", dump);
    assert!(dump.contains("for Circle→"), "dump: {}", dump);
}

// ── Backward compatibility ────────────────────────────────────────────────────

#[test]
fn test_no_traits_backward_compatible() {
    let hir = lower_source("fn add(a: Int, b: Int) -> Int { a + b }");
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    assert_eq!(hir.items.len(), 1);
    match &hir.items[0] {
        Item::FnDef(f) => assert_eq!(f.name, "add"),
        _ => panic!("expected FnDef"),
    }
}

// ── @lang on traits ───────────────────────────────────────────────────────────

#[test]
fn test_trait_with_lang_tag() {
    let hir = lower_source("@lang(\"my_trait\") trait MyTrait { fn m() -> Int; }");
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    match &hir.items[0] {
        Item::TraitDef(t) => {
            assert_eq!(t.lang_tag.as_deref(), Some("my_trait"));
        }
        _ => panic!("expected TraitDef"),
    }
}

#[test]
fn test_trait_method_with_lang_tag() {
    let hir =
        lower_source("trait MyTrait { @lang(\"my_method\") fn m() -> Int; fn n() -> Int { 0 } }");
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    match &hir.items[0] {
        Item::TraitDef(t) => {
            assert_eq!(t.methods.len(), 2);
            assert_eq!(t.methods[0].lang_tag.as_deref(), Some("my_method"));
            assert_eq!(t.methods[1].lang_tag.as_deref(), None);
        }
        _ => panic!("expected TraitDef"),
    }
}

#[test]
fn test_trait_with_lang_serializes() {
    let hir = lower_source("@lang(\"my_trait\") trait MyTrait { @lang(\"m\") fn m() -> Int; }");
    let dump = serialize(&hir);
    assert!(dump.contains("@lang=\"my_trait\""), "dump: {dump}");
    assert!(dump.contains("@lang=\"m\""), "dump: {dump}");
}

// ── Trait method own type params ──────────────────────────────────────────────

#[test]
fn test_trait_method_with_own_type_param() {
    let hir = lower_source("trait Convert { fn to<S>(self) -> S; }");
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    match &hir.items[0] {
        Item::TraitDef(t) => {
            assert_eq!(t.methods.len(), 1);
            assert_eq!(t.methods[0].type_params.len(), 1);
            assert_eq!(t.methods[0].type_params[0].name, "S");
        }
        _ => panic!("expected TraitDef"),
    }
}

#[test]
fn test_trait_method_type_param_in_serializer() {
    let hir = lower_source("trait Convert { fn to<S>(self) -> S; }");
    let dump = serialize(&hir);
    assert!(dump.contains("<S>"), "expected <S> in dump: {dump}");
}

#[test]
fn test_trait_method_type_param_shadows_trait_param() {
    let hir = lower_source("trait Foo<T> { fn bar<T>(self) -> T; }");
    let has_duplicate = hir.diagnostics.iter().any(
        |d| matches!(d, lower::HirDiagnostic::DuplicateDefinition { name, .. } if name == "T"),
    );
    assert!(
        has_duplicate,
        "expected DuplicateDefinition for T shadowing, got: {:?}",
        hir.diagnostics
    );
}

#[test]
fn test_trait_method_type_param_no_shadow_is_ok() {
    let hir = lower_source("trait Foo<T> { fn bar<S>(self) -> S; }");
    assert!(
        hir.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        hir.diagnostics
    );
}
