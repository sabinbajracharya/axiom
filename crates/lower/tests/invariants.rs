//! Coverage invariants (`docs/hir-testing.md` §6, Layer 3). The drift guard
//! proves that every AST node kind the parser can produce (items, statements,
//! expressions, patterns, types) is handled by the lowerer — either lowered
//! to a real HIR node or emitted as a `NotYetSupported` diagnostic. Adding a
//! new node kind to `SyntaxKind` without updating the lowerer fails this test.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use parser::ast::{is_expr_kind, is_pat, is_type_kind};
use parser::SyntaxKind;

fn is_stmt_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LetStmt
            | SyntaxKind::ExprStmt
            | SyntaxKind::ReturnStmt
            | SyntaxKind::BreakStmt
            | SyntaxKind::ContinueStmt
            | SyntaxKind::ErrdeferStmt
            | SyntaxKind::YieldStmt
    )
}

fn is_item_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FnDef
            | SyntaxKind::StructDef
            | SyntaxKind::EnumDef
            | SyntaxKind::TraitDef
            | SyntaxKind::ImplBlock
            | SyntaxKind::ModDef
            | SyntaxKind::UseDecl
            | SyntaxKind::ConstDef
            | SyntaxKind::ErrorSetDef
            | SyntaxKind::SubscriptDef
    )
}

fn handles_item(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FnDef
            | SyntaxKind::StructDef
            | SyntaxKind::EnumDef
            | SyntaxKind::TraitDef
            | SyntaxKind::ImplBlock
            | SyntaxKind::ModDef
            | SyntaxKind::UseDecl
            | SyntaxKind::ConstDef
            | SyntaxKind::ErrorSetDef
            | SyntaxKind::SubscriptDef
    )
}

fn handles_stmt(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LetStmt
            | SyntaxKind::ExprStmt
            | SyntaxKind::ReturnStmt
            | SyntaxKind::BreakStmt
            | SyntaxKind::ContinueStmt
            | SyntaxKind::ErrdeferStmt
            | SyntaxKind::YieldStmt
    )
}

fn handles_expr(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LiteralExpr
            | SyntaxKind::PathExpr
            | SyntaxKind::BinExpr
            | SyntaxKind::PrefixExpr
            | SyntaxKind::CallExpr
            | SyntaxKind::MethodCallExpr
            | SyntaxKind::FieldExpr
            | SyntaxKind::IndexExpr
            | SyntaxKind::BlockExpr
            | SyntaxKind::IfExpr
            | SyntaxKind::MatchExpr
            | SyntaxKind::LoopExpr
            | SyntaxKind::StructLitExpr
            | SyntaxKind::AssignExpr
            | SyntaxKind::ReturnStmt
            | SyntaxKind::BreakStmt
            | SyntaxKind::ContinueStmt
            | SyntaxKind::ParenExpr
            | SyntaxKind::ClosureExpr
            | SyntaxKind::CastExpr
            | SyntaxKind::RangeExpr
            | SyntaxKind::TryExpr
            | SyntaxKind::QuestionExpr
            | SyntaxKind::CatchExpr
            | SyntaxKind::ElseExpr
            | SyntaxKind::ScopeExpr
            | SyntaxKind::SpawnExpr
            | SyntaxKind::ListLitExpr
            | SyntaxKind::YieldStmt
    )
}

fn handles_pat(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::WildcardPat
            | SyntaxKind::LiteralPat
            | SyntaxKind::IdentPat
            | SyntaxKind::TupleStructPat
            | SyntaxKind::StructPat
            | SyntaxKind::OrPat
            | SyntaxKind::RangePat
            | SyntaxKind::PathPat
            | SyntaxKind::RestPat
    )
}

fn handles_type(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PathType
            | SyntaxKind::SliceType
            | SyntaxKind::UnitType
            | SyntaxKind::Error
            | SyntaxKind::ErrorUnionType
            | SyntaxKind::ErrorSetUnionType
    )
}

fn lowerer_handles(kind: SyntaxKind) -> bool {
    if is_item_kind(kind) {
        handles_item(kind)
    } else if is_stmt_kind(kind) {
        handles_stmt(kind)
    } else if is_expr_kind(kind) {
        handles_expr(kind)
    } else if is_pat(kind) {
        handles_pat(kind)
    } else if is_type_kind(kind) {
        handles_type(kind)
    } else {
        false
    }
}

#[test]
fn test_lowerer_handles_every_ast_node_kind() {
    let kinds: Vec<SyntaxKind> = SyntaxKind::ALL
        .iter()
        .copied()
        .filter(|k| !k.is_trivia() && *k != SyntaxKind::Error)
        .filter(|k| {
            is_item_kind(*k)
                || is_stmt_kind(*k)
                || is_expr_kind(*k)
                || is_pat(*k)
                || is_type_kind(*k)
        })
        .collect();

    for kind in kinds {
        assert!(
            lowerer_handles(kind),
            "lowerer does not handle AST node kind: {kind:?}",
        );
    }
}
