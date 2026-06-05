//! Slice type `[T]` HIR lowering tests. A slice is the byte-buffer / array
//! element view introduced for the extern boundary (`let buf: [U8]`) — see
//! `docs/extern-buffers-and-path-unification.md`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_hir::{serialize, HirTy, Item};
use axiom_parser::ast::{AstNode, SourceFile};
use axiom_parser::parse;

fn lower_source(source: &str) -> axiom_hir::Hir {
    let result = parse(source);
    let root = SourceFile::cast(result.tree).unwrap();
    axiom_hir::lower(&root, source, None)
}

#[test]
fn test_slice_param_lowers_to_slice_ty() {
    let hir = lower_source("fn f(let buf: [U8]) { }");
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    match &hir.items[0] {
        Item::FnDef(f) => {
            let ty = f.params[0].ty.as_ref().expect("param has a type");
            match ty {
                HirTy::Slice(elem) => match elem.as_ref() {
                    HirTy::Named(_) => {}
                    other => panic!("expected Named element, got {other:?}"),
                },
                other => panic!("expected Slice, got {other:?}"),
            }
        }
        _ => panic!("expected FnDef"),
    }
}

#[test]
fn test_slice_type_serializes_with_brackets() {
    let hir = lower_source("fn f(let buf: [U8]) { }");
    let dump = serialize(&hir);
    assert!(
        dump.contains("buf: [U8"),
        "expected `[U8]` in dump:\n{dump}"
    );
}

#[test]
fn test_nested_slice_type_lowers() {
    let hir = lower_source("fn f(let m: [[Int]]) { }");
    assert!(
        hir.diagnostics.is_empty(),
        "unexpected: {:?}",
        hir.diagnostics
    );
    match &hir.items[0] {
        Item::FnDef(f) => {
            let ty = f.params[0].ty.as_ref().expect("param has a type");
            match ty {
                HirTy::Slice(outer) => assert!(
                    matches!(outer.as_ref(), HirTy::Slice(_)),
                    "expected nested Slice, got {outer:?}"
                ),
                other => panic!("expected Slice, got {other:?}"),
            }
        }
        _ => panic!("expected FnDef"),
    }
}
