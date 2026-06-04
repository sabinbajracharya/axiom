//! Typed AST views over the lossless CST (`docs/parser-testing.md` §2.4).
//!
//! Each struct is a thin wrapper over `SyntaxNode` — no data of its own.
//! `AstNode` is the common interface; each view adds accessor methods that
//! navigate to immediate children by kind, skipping trivia.
//!
//! The compiler consumes this layer and never sees trivia; the formatter
//! consumes the raw red tree and sees everything.
//!
//! The views are grouped into submodules by family (items, statements,
//! expressions, patterns, types, …) and re-exported here so the public surface
//! is `ast::FnDef`, `ast::LetStmt`, … regardless of which file a view lives in.
//! This module owns the `AstNode` trait, the shared navigation helpers, and the
//! cross-family kind classifiers (`is_pat` / `is_type_kind` / `is_expr_kind`).

pub(crate) use crate::syntax::{SyntaxElement, SyntaxNode, SyntaxToken};
pub(crate) use crate::syntax_kind::SyntaxKind;

mod expr;
mod expr_flow;
mod expr_part;
mod item;
mod item_part;
mod name;
mod pattern;
mod stmt;
mod ty;

pub use expr::*;
pub use expr_flow::*;
pub use expr_part::*;
pub use item::*;
pub use item_part::*;
pub use name::*;
pub use pattern::*;
pub use stmt::*;
pub use ty::*;

// ── Trait ────────────────────────────────────────────────────────────────────

/// The common interface for every typed AST view.
pub trait AstNode: Sized {
    /// Whether a `SyntaxNode` of this `kind` can be wrapped as `Self`.
    fn can_cast(kind: SyntaxKind) -> bool;
    /// Wrap `node` as `Self`, returning `None` if the kind does not match.
    fn cast(node: SyntaxNode) -> Option<Self>;
    /// The underlying red-tree node.
    fn syntax(&self) -> &SyntaxNode;
}

// ── Navigation helpers ────────────────────────────────────────────────────────

/// First child node that casts successfully to `N`.
pub(crate) fn child_node<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
    parent.child_nodes().into_iter().find_map(N::cast)
}

/// All child nodes that cast successfully to `N`.
pub(crate) fn child_nodes_of<N: AstNode>(parent: &SyntaxNode) -> Vec<N> {
    parent
        .child_nodes()
        .into_iter()
        .filter_map(N::cast)
        .collect()
}

/// First non-trivia token child with a specific `kind`.
pub(crate) fn child_token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    parent.children().into_iter().find_map(|e| match e {
        SyntaxElement::Token(t) if t.kind() == kind => Some(t),
        _ => None,
    })
}

/// First non-trivia token child, any kind.
pub(crate) fn first_token(parent: &SyntaxNode) -> Option<SyntaxToken> {
    parent.children().into_iter().find_map(|e| match e {
        SyntaxElement::Token(t) if !t.kind().is_trivia() => Some(t),
        _ => None,
    })
}

/// First child node whose kind is in the pattern family.
pub fn child_pat_node(parent: &SyntaxNode) -> Option<SyntaxNode> {
    parent.child_nodes().into_iter().find(|n| is_pat(n.kind()))
}

/// First child node whose kind is in the type family.
pub(crate) fn child_type_node(parent: &SyntaxNode) -> Option<SyntaxNode> {
    parent
        .child_nodes()
        .into_iter()
        .find(|n| is_type_kind(n.kind()))
}

/// First child node whose kind is in the expression family.
pub(crate) fn child_expr_node(parent: &SyntaxNode) -> Option<SyntaxNode> {
    parent
        .child_nodes()
        .into_iter()
        .find(|n| is_expr_kind(n.kind()))
}

/// All child nodes whose kind is in the expression family.
pub(crate) fn child_expr_nodes(parent: &SyntaxNode) -> Vec<SyntaxNode> {
    parent
        .child_nodes()
        .into_iter()
        .filter(|n| is_expr_kind(n.kind()))
        .collect()
}

pub fn is_pat(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::WildcardPat
            | SyntaxKind::LiteralPat
            | SyntaxKind::IdentPat
            | SyntaxKind::TupleStructPat
            | SyntaxKind::StructPat
            | SyntaxKind::PathPat
            | SyntaxKind::OrPat
            | SyntaxKind::RestPat
            | SyntaxKind::RangePat
    )
}

pub fn is_type_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PathType
            | SyntaxKind::ErrorUnionType
            | SyntaxKind::ErrorSetUnionType
            | SyntaxKind::UnitType
            | SyntaxKind::Error
    )
}

pub fn is_expr_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BlockExpr
            | SyntaxKind::LiteralExpr
            | SyntaxKind::PathExpr
            | SyntaxKind::BinExpr
            | SyntaxKind::PrefixExpr
            | SyntaxKind::CallExpr
            | SyntaxKind::MethodCallExpr
            | SyntaxKind::FieldExpr
            | SyntaxKind::IndexExpr
            | SyntaxKind::ParenExpr
            | SyntaxKind::IfExpr
            | SyntaxKind::MatchExpr
            | SyntaxKind::LoopExpr
            | SyntaxKind::ClosureExpr
            | SyntaxKind::StructLitExpr
            | SyntaxKind::CastExpr
            | SyntaxKind::RangeExpr
            | SyntaxKind::TryExpr
            | SyntaxKind::AssignExpr
            | SyntaxKind::CatchExpr
            | SyntaxKind::ScopeExpr
            | SyntaxKind::SpawnExpr
            | SyntaxKind::ListLitExpr
            | SyntaxKind::ReturnStmt
            | SyntaxKind::BreakStmt
            | SyntaxKind::ContinueStmt
            | SyntaxKind::YieldStmt
    )
}

#[cfg(test)]
mod tests;
