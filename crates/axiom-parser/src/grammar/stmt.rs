//! Statement & block grammar (`DESIGN_SPEC.md` §5, §7.4). A block is a sequence
//! of statements; statements are `val`/`var` bindings, `errdefer`, or
//! expression-statements. `return`/`break`/`continue` are expressions (they
//! parse in `expr.rs`), so they flow through `expr_stmt` uniformly.

use super::expr::expr;
use super::pattern::pattern;
use super::ty::ty;
use crate::parser::{CompletedMarker, Parser};
use crate::syntax_kind::SyntaxKind as K;

/// `{ stmt* }` — also the block-as-expression form (§7.4). Every iteration
/// consumes at least one token (each statement bumps or recovers), so the loop
/// terminates.
pub(crate) fn block(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.expect(K::LBrace);
    // Blocks are a nesting point too; count them toward the recursion guard so
    // deeply nested `{ ... }` recovers instead of overflowing the stack.
    if p.enter_recursion() {
        while !p.at(K::RBrace) && !p.at_end() {
            let before = p.pos();
            stmt(p);
            // Recovery may decline to consume a closing delimiter that an
            // enclosing construct owns (`err_recover`); if a statement made no
            // progress, break so that closer bubbles out instead of spinning.
            if p.pos() == before {
                break;
            }
        }
    } else {
        p.error("block nesting too deep");
        while !p.at(K::RBrace) && !p.at_end() {
            p.bump();
        }
    }
    p.leave_recursion();
    p.expect(K::RBrace);
    m.complete(p, K::BlockExpr)
}

fn stmt(p: &mut Parser) {
    match p.current() {
        K::KwVal | K::KwVar => let_stmt(p),
        K::KwErrdefer => errdefer_stmt(p),
        _ => expr_stmt(p),
    }
}

/// `val|var pattern (: Type)? (= expr)?` — the binding axis (§5.1). A pattern
/// (not just a name) is accepted so destructuring bindings parse.
fn let_stmt(p: &mut Parser) {
    let m = p.start();
    p.bump(); // val / var
    pattern(p);
    if p.eat(K::Colon) {
        ty(p);
    }
    if p.eat(K::Eq) {
        expr(p);
    }
    p.eat(K::Semicolon);
    m.complete(p, K::LetStmt);
}

/// `errdefer stmt` (§6.4) — error-path cleanup.
fn errdefer_stmt(p: &mut Parser) {
    let m = p.start();
    p.bump(); // errdefer
    expr(p);
    p.eat(K::Semicolon);
    m.complete(p, K::ErrdeferStmt);
}

fn expr_stmt(p: &mut Parser) {
    let m = p.start();
    expr(p);
    p.eat(K::Semicolon);
    m.complete(p, K::ExprStmt);
}
