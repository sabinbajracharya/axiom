//! Block tail-expression lowering: a block's final expression is its value
//! only when it has no trailing `;` (DESIGN_SPEC §16). A trailing `;` discards
//! the value, so the block lowers with no `tail` and evaluates to `()`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hir::{serialize, Item};
use parser::ast::{AstNode, SourceFile};
use parser::parse;

fn lower_source(source: &str) -> hir::Hir {
    let result = parse(source);
    let root = SourceFile::cast(result.tree).unwrap();
    hir::lower(&root, source, None)
}

fn body_has_tail(src: &str) -> bool {
    let hir = lower_source(src);
    match &hir.items[0] {
        Item::FnDef(f) => f.body.tail.is_some(),
        _ => panic!("expected FnDef"),
    }
}

#[test]
fn test_final_expr_without_semicolon_is_tail() {
    assert!(body_has_tail("fn f() -> Int { 1 + 2 }"));
}

#[test]
fn test_final_expr_with_semicolon_is_discarded() {
    // The trailing `;` makes the block evaluate to () — no tail.
    assert!(!body_has_tail("fn f() { 1 + 2; }"));
}

#[test]
fn test_semicolon_terminated_call_is_a_statement() {
    let hir = lower_source("fn f() { g(); }");
    let dump = serialize(&hir);
    assert!(dump.contains("ExprStmt"), "expected ExprStmt, got:\n{dump}");
    assert!(!dump.contains("tail:"), "should have no tail:\n{dump}");
}
