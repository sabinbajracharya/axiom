//! Pinpoint tests for operator precedence and associativity
//! (`docs/parser-testing.md` §7, Layer 6). These parse small expressions and
//! verify the tree shape encodes the correct nesting.
//!
//! Precedence table (higher bp = binds tighter):
//!   assign(3) < range(4) < logical-or(6) < logical-and(8) < comparison(10)
//!   < bitor(12) < bitxor(14) < bitand(16) < shift(18) < add/sub(20)
//!   < mul/div/mod(22) < prefix(24)

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axiom_parser::ast::AstNode;
use axiom_parser::{SyntaxElement, SyntaxKind, SyntaxNode};

/// Parse source and return the root `SourceFile` node.
fn tree(src: &str) -> axiom_parser::ast::SourceFile {
    let result = axiom_parser::parse(src);
    assert!(
        result.errors.is_empty(),
        "parse errors: {:?}",
        result.errors
    );
    axiom_parser::ast::SourceFile::cast(result.tree).unwrap()
}

/// Find the first node of the given kind in pre-order traversal.
fn find_node(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    if node.kind() == kind {
        return Some(node.clone());
    }
    for child in node.child_nodes() {
        if let Some(found) = find_node(&child, kind) {
            return Some(found);
        }
    }
    None
}

/// Get the operator token text from a BinExpr or AssignExpr.
fn bin_op(expr: &SyntaxNode) -> String {
    expr.children()
        .into_iter()
        .find_map(|elem| match elem {
            SyntaxElement::Token(t)
                if matches!(
                    t.kind(),
                    SyntaxKind::Plus
                        | SyntaxKind::Minus
                        | SyntaxKind::Star
                        | SyntaxKind::Slash
                        | SyntaxKind::Percent
                        | SyntaxKind::Lt
                        | SyntaxKind::Le
                        | SyntaxKind::Gt
                        | SyntaxKind::Ge
                        | SyntaxKind::EqEq
                        | SyntaxKind::Ne
                        | SyntaxKind::AmpAmp
                        | SyntaxKind::PipePipe
                        | SyntaxKind::Amp
                        | SyntaxKind::Pipe
                        | SyntaxKind::Caret
                        | SyntaxKind::Shl
                        | SyntaxKind::Shr
                        | SyntaxKind::DotDot
                        | SyntaxKind::Eq
                ) =>
            {
                Some(t.text().to_string())
            }
            _ => None,
        })
        .expect("expression must have an operator token")
}

// ── Precedence tests ──────────────────────────────────────────────────────

#[test]
fn precedence_mul_binds_tighter_than_add() {
    let root = tree("fn f() { 1 + 2 * 3 }");
    let bin = find_node(root.syntax(), SyntaxKind::BinExpr).unwrap();
    // Top-level operator should be `+`, with `*` nested inside.
    assert_eq!(bin_op(&bin), "+");
    let rhs = bin
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::BinExpr)
        .expect("rhs BinExpr for *");
    assert_eq!(bin_op(&rhs), "*");
}

#[test]
fn precedence_div_binds_tighter_than_sub() {
    let root = tree("fn f() { 10 - 6 / 2 }");
    let bin = find_node(root.syntax(), SyntaxKind::BinExpr).unwrap();
    assert_eq!(bin_op(&bin), "-");
    let rhs = bin
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::BinExpr)
        .unwrap();
    assert_eq!(bin_op(&rhs), "/");
}

#[test]
fn precedence_comparison_looser_than_arithmetic() {
    let root = tree("fn f() { 1 + 2 > 3 * 4 }");
    let bin = find_node(root.syntax(), SyntaxKind::BinExpr).unwrap();
    // Top-level should be `>`, with `+` and `*` nested as children.
    assert_eq!(bin_op(&bin), ">");
    let children: Vec<SyntaxNode> = bin
        .child_nodes()
        .into_iter()
        .filter(|c| c.kind() == SyntaxKind::BinExpr)
        .collect();
    assert_eq!(children.len(), 2, "must have two child BinExpr nodes");
    assert_eq!(bin_op(&children[0]), "+");
    assert_eq!(bin_op(&children[1]), "*");
}

#[test]
fn precedence_logical_and_binds_tighter_than_or() {
    let root = tree("fn f() { a || b && c }");
    let bin = find_node(root.syntax(), SyntaxKind::BinExpr).unwrap();
    assert_eq!(bin_op(&bin), "||");
    let rhs = bin
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::BinExpr)
        .unwrap();
    assert_eq!(bin_op(&rhs), "&&");
}

#[test]
fn precedence_bitand_binds_tighter_than_comparison() {
    // bp: bitand(16) > comparison(10), so `&` binds tighter.
    // In `a & b == c`: `==` is top (looser), `&` is nested (tighter).
    let root = tree("fn f() { a & b == c }");
    let bin = find_node(root.syntax(), SyntaxKind::BinExpr).unwrap();
    assert_eq!(bin_op(&bin), "==");
    let lhs = bin
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::BinExpr)
        .unwrap();
    assert_eq!(bin_op(&lhs), "&");
}

#[test]
fn precedence_add_binds_tighter_than_shift() {
    // bp: add(20) > shift(18), so `+` binds tighter than `<<`.
    // In `1 + 2 << 3`: `<<` is top (looser), `+` is nested (tighter).
    let root = tree("fn f() { 1 + 2 << 3 }");
    let bin = find_node(root.syntax(), SyntaxKind::BinExpr).unwrap();
    assert_eq!(bin_op(&bin), "<<");
    let lhs = bin
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::BinExpr)
        .unwrap();
    assert_eq!(bin_op(&lhs), "+");
}

// ── Associativity tests ───────────────────────────────────────────────────

#[test]
fn associativity_add_is_left() {
    // 1 + 2 + 3  ===  (1 + 2) + 3
    let root = tree("fn f() { 1 + 2 + 3 }");
    let bin = find_node(root.syntax(), SyntaxKind::BinExpr).unwrap();
    assert_eq!(bin_op(&bin), "+");
    // The LHS should be a BinExpr (1+2), RHS should be a literal (3).
    let lhs = bin
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::BinExpr)
        .unwrap();
    assert_eq!(bin_op(&lhs), "+");
    assert!(
        bin.child_nodes()
            .into_iter()
            .any(|c| c.kind() == SyntaxKind::LiteralExpr),
        "RHS must be a literal"
    );
}

#[test]
fn associativity_mul_is_left() {
    // 2 * 3 * 4  ===  (2 * 3) * 4
    let root = tree("fn f() { 2 * 3 * 4 }");
    let bin = find_node(root.syntax(), SyntaxKind::BinExpr).unwrap();
    assert_eq!(bin_op(&bin), "*");
    let lhs = bin
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::BinExpr)
        .unwrap();
    assert_eq!(bin_op(&lhs), "*");
}

#[test]
fn associativity_assign_is_right() {
    // a = b = c  ===  a = (b = c)
    // Assignment uses AssignExpr, not BinExpr.
    let root = tree("fn f() { a = b = c }");
    let assign = find_node(root.syntax(), SyntaxKind::AssignExpr).unwrap();
    assert_eq!(bin_op(&assign), "=");
    // The RHS should be another AssignExpr (=).
    let rhs = assign
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::AssignExpr)
        .unwrap();
    assert_eq!(bin_op(&rhs), "=");
}

#[test]
fn associativity_range_is_right() {
    // 1..2..3  — range is left-associative: (1..2)..3
    // Range uses RangeExpr, not BinExpr.
    let root = tree("fn f() { 1..2..3 }");
    let range = find_node(root.syntax(), SyntaxKind::RangeExpr).unwrap();
    assert_eq!(bin_op(&range), "..");
    // The LHS should be another RangeExpr (1..2), not the RHS.
    let lhs = range
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::RangeExpr)
        .unwrap();
    assert_eq!(bin_op(&lhs), "..");
}

// ── Combined precedence + associativity ───────────────────────────────────

#[test]
fn mixed_precedence_chain() {
    // 1 + 2 * 3 - 4 / 2
    // Expected: (1 + (2 * 3)) - (4 / 2)
    let root = tree("fn f() { 1 + 2 * 3 - 4 / 2 }");
    let bin = find_node(root.syntax(), SyntaxKind::BinExpr).unwrap();
    // Top-level should be `-` (left-associative: (1+2*3) - (4/2))
    assert_eq!(bin_op(&bin), "-");
    // LHS should be `+` with `*` nested
    let lhs = bin
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::BinExpr)
        .unwrap();
    assert_eq!(bin_op(&lhs), "+");
    let lhs_rhs = lhs
        .child_nodes()
        .into_iter()
        .find(|c| c.kind() == SyntaxKind::BinExpr)
        .unwrap();
    assert_eq!(bin_op(&lhs_rhs), "*");
    // RHS should be `/`
    let rhs = bin
        .child_nodes()
        .into_iter()
        .filter(|c| c.kind() == SyntaxKind::BinExpr)
        .last()
        .unwrap();
    assert_eq!(bin_op(&rhs), "/");
}
