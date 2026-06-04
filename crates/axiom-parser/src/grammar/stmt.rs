//! Statement & block grammar (`DESIGN_SPEC.md` §5, §7.4). A block is a sequence
//! of statements; statements are `val`/`var` bindings, `errdefer`, or
//! expression-statements. `return`/`break`/`continue` are expressions (they
//! parse in `expr.rs`), so they flow through `expr_stmt` uniformly.

use super::expr::{expr, EXPR_START};
use super::pattern::pattern;
use super::ty::ty;
use crate::parser::{CompletedMarker, Parser};
use crate::syntax_kind::SyntaxKind as K;

/// Statement first-set: every expression start (an expression is a statement),
/// plus the binding/cleanup keywords that only ever begin a statement. This is
/// the resync target for block recovery — a token here begins the *next*
/// statement, so garbage is skipped up to it (see `block`).
const STMT_ONLY_START: &[K] = &[K::KwVal, K::KwVar, K::KwErrdefer, K::KwYield];

/// Whether the current token can begin a statement (so the block loop should try
/// to parse one rather than treating it as garbage).
fn at_stmt_start(p: &Parser) -> bool {
    p.at_any(STMT_ONLY_START) || p.at_any(EXPR_START)
}

/// `{ stmt* }` — also the block-as-expression form (§7.4). Every iteration
/// consumes at least one token (a statement bumps, or `recover_to` absorbs a
/// garbage run), so the loop terminates.
pub(crate) fn block(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.expect(K::LBrace);
    // Blocks are a nesting point too; count them toward the recursion guard so
    // deeply nested `{ ... }` recovers instead of overflowing the stack.
    if p.enter_recursion() {
        while !p.at(K::RBrace) && !p.at_end() {
            if at_stmt_start(p) {
                let before = p.pos();
                stmt(p);
                // A statement that made no progress means a leaf recovery
                // declined a closer an enclosing construct owns; break so it
                // bubbles out instead of spinning.
                if p.pos() == before {
                    break;
                }
            } else if !p.recover_to("expected a statement", at_stmt_start) {
                // Already at a claimed closer or end — let the owner have it.
                break;
            }
            // Otherwise `recover_to` absorbed a garbage run into one Error node
            // and resynced to the next statement; loop on.
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
        K::KwYield => yield_stmt(p),
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

/// `yield expr` — subscript body value producer.
fn yield_stmt(p: &mut Parser) {
    let m = p.start();
    p.bump(); // yield
    expr(p);
    p.eat(K::Semicolon);
    m.complete(p, K::YieldStmt);
}

fn expr_stmt(p: &mut Parser) {
    let m = p.start();
    expr(p);
    p.eat(K::Semicolon);
    m.complete(p, K::ExprStmt);
}

#[cfg(test)]
mod tests {
    // Tests legitimately panic/assert on failure. RUST_CONVENTIONS §3.4.
    #![allow(clippy::panic)]
    use crate::{parse, serialize, SyntaxKind};

    /// A representative source snippet for each `EXPR_START` kind. The kinds are
    /// statement starts (an expression is a statement), so each must parse as a
    /// statement without tripping block resync ("expected a statement"). This is
    /// the mechanized guard that `EXPR_START` mirrors the `expr::primary` /
    /// `expr::lhs_inner` dispatch — see the note on `EXPR_START`.
    fn snippet_for(kind: SyntaxKind) -> &'static str {
        use SyntaxKind as K;
        match kind {
            K::IntLit => "1",
            K::FloatLit => "1.0",
            K::ByteLit => "b'a'",
            K::StrLit => "\"s\"",
            K::KwTrue => "true",
            K::KwFalse => "false",
            K::Ident => "x",
            K::KwSelf => "self",
            K::KwSelfType => "Self",
            K::LParen => "(x)",
            K::LBracket => "[x]",
            K::LBrace => "{ }",
            K::KwIf => "if x { }",
            K::KwMatch => "match x { }",
            K::KwLoop => "loop { }",
            K::Label => "'l: loop { }",
            K::KwScope => "scope { }",
            K::Pipe => "|a| a",
            K::PipePipe => "|| x",
            K::Minus => "-x",
            K::Bang => "!x",
            K::KwTry => "try x",
            K::KwReturn => "return",
            K::KwBreak => "break",
            K::KwContinue => "continue",
            other => panic!("EXPR_START gained {other:?} with no test snippet — add one"),
        }
    }

    #[test]
    fn test_expr_start_matches_primary_dispatch() {
        for &kind in super::EXPR_START {
            let src = format!("fn f() {{ {} }}", snippet_for(kind));
            let result = parse(&src);
            let resync: Vec<_> = result
                .errors
                .iter()
                .filter(|e| e.message.contains("expected a statement"))
                .collect();
            assert!(
                resync.is_empty(),
                "{kind:?} is in EXPR_START but `primary`/`lhs_inner` did not \
                 accept it as a statement (got resync diagnostics: {resync:?}) — \
                 EXPR_START has drifted from the dispatch",
            );
        }
    }

    #[test]
    fn test_statement_garbage_run_collapses_to_one_error() {
        // A run of tokens that can't begin a statement must become ONE Error node
        // with ONE diagnostic — resync to the next statement — not one per token.
        let result = parse("fn f() {\n    @ @ @ val x = 1\n}\n");
        let resync: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.message.contains("expected a statement"))
            .collect();
        assert_eq!(
            resync.len(),
            1,
            "garbage run should yield exactly one diagnostic, got {:?}",
            result.errors
        );
        // ...and exactly one Error node in the dump (the collapsed run); the
        // following `val x = 1` recovers and parses as a normal statement.
        let dump = serialize(&result.tree);
        assert_eq!(
            dump.matches("Error @").count(),
            1,
            "garbage run should collapse to one Error node:\n{dump}"
        );
        assert!(
            dump.contains("LetStmt @"),
            "val binding should still parse:\n{dump}"
        );
    }
}
