//! Coverage invariants (`docs/parser-testing.md` §4) — the parser's mechanical
//! "nothing was missed" proof, defined **once** and called by golden tests, unit
//! tests, and the fuzzer alike (DRY: never re-implemented).
//!
//! - `reconstruct` — the tree rebuilds the source byte-for-byte (losslessness).
//! - `spans_well_formed` — the tree *shape* is sound (nesting + contiguity).
//! - `every_token_present` — every significant lexer token landed in the tree,
//!   in order, exactly once.
//!
//! All pure; they take a tree and return data or a typed error, never panic.

use crate::syntax::{SyntaxElement, SyntaxNode};
use crate::syntax_kind::SyntaxKind;
use axiom_lexer::Token;

/// A structural defect found by `spans_well_formed`.
#[derive(Debug, PartialEq)]
pub enum SpanError {
    /// The root span did not cover `[0, source_len)`.
    RootNotWholeSource { got_hi: usize, source_len: usize },
    /// A child poked outside its parent's span.
    ChildEscapesParent {
        parent: SyntaxKind,
        child: SyntaxKind,
    },
    /// Two adjacent siblings were not contiguous (a gap or an overlap).
    SiblingsNotContiguous { prev_hi: usize, next_lo: usize },
    /// A node's span did not equal the union of its children's spans.
    NodeSpanMismatch { node: SyntaxKind },
}

/// A coverage defect found by `every_token_present`.
#[derive(Debug, PartialEq)]
pub enum CoverageError {
    /// The significant (non-trivia) text in the tree did not match the lexer's.
    /// Compared as concatenated text rather than per-token because the parser may
    /// legitimately split one source token into several leaves (`>>` → `>` `>`)
    /// or join none — what must hold is that every significant byte is present,
    /// in order, and not misclassified as trivia.
    SignificantTextMismatch { tree: String, lexer: String },
}

/// Concatenate every leaf token's text, left to right. **Invariant:**
/// `reconstruct(root) == source` on every input — the losslessness guarantee.
pub fn reconstruct(root: &SyntaxNode) -> String {
    root.tokens().iter().map(|t| t.text()).collect()
}

/// Check the tree shape: root covers the whole source, children nest inside and
/// tile their parent without gaps or overlaps, and each node's span is the union
/// of its children's.
pub fn spans_well_formed(root: &SyntaxNode, source_len: usize) -> Result<(), SpanError> {
    let span = root.span();
    if span.lo != 0 || span.hi != source_len {
        return Err(SpanError::RootNotWholeSource {
            got_hi: span.hi,
            source_len,
        });
    }
    check_node(root)
}

/// Recursive per-node shape check (one node's children), then recurse.
fn check_node(node: &SyntaxNode) -> Result<(), SpanError> {
    let children = node.children();
    let parent = node.span();
    let mut cursor = parent.lo;
    for child in &children {
        let cspan = child.span();
        if cspan.lo < parent.lo || cspan.hi > parent.hi {
            return Err(SpanError::ChildEscapesParent {
                parent: node.kind(),
                child: child.kind(),
            });
        }
        if cspan.lo != cursor {
            return Err(SpanError::SiblingsNotContiguous {
                prev_hi: cursor,
                next_lo: cspan.lo,
            });
        }
        cursor = cspan.hi;
    }
    // After walking all children, the cursor must reach the parent's end —
    // unless the node is a childless leaf-holder (cursor stayed at lo == hi only
    // for an empty node). A node with children must tile exactly to `hi`.
    if !children.is_empty() && cursor != parent.hi {
        return Err(SpanError::NodeSpanMismatch { node: node.kind() });
    }
    for child in children {
        if let SyntaxElement::Node(n) = child {
            check_node(&n)?;
        }
    }
    Ok(())
}

/// Assert the tree's significant (non-trivia) text equals the lexer's
/// significant text, concatenated in order. Proves no significant byte was
/// dropped, duplicated, reordered, or misclassified as trivia during
/// parsing/recovery. Compared as concatenated text (not per-token) because the
/// parser may split one source token into several leaves (`>>` → `>` `>`); what
/// must hold is byte-level coverage of the significant stream. `Eof` and trivia
/// are excluded on both sides.
pub fn every_token_present(root: &SyntaxNode, lexer_tokens: &[Token]) -> Result<(), CoverageError> {
    let tree: String = root
        .tokens()
        .iter()
        .filter(|t| !t.kind().is_trivia())
        .map(|t| t.text())
        .collect();
    let lexer: String = lexer_tokens
        .iter()
        .filter(|t| !t.kind.is_trivia() && t.kind != axiom_lexer::TokenKind::Eof)
        .map(|t| t.text.as_str())
        .collect();
    if tree != lexer {
        return Err(CoverageError::SignificantTextMismatch { tree, lexer });
    }
    Ok(())
}

/// Run all three coverage invariants. This is what the fuzzer and every golden
/// test call. Returns a human-readable description of the first failure.
pub fn check_all(root: &SyntaxNode, source: &str, lexer_tokens: &[Token]) -> Result<(), String> {
    let rebuilt = reconstruct(root);
    if rebuilt != source {
        return Err(format!(
            "reconstruct mismatch: tree rebuilt {} bytes, source is {} bytes",
            rebuilt.len(),
            source.len()
        ));
    }
    spans_well_formed(root, source.len()).map_err(|e| format!("span error: {e:?}"))?;
    every_token_present(root, lexer_tokens).map_err(|e| format!("coverage error: {e:?}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::green::GreenNodeBuilder;

    fn sample() -> SyntaxNode {
        let mut b = GreenNodeBuilder::new();
        b.start_node(SyntaxKind::SourceFile);
        b.start_node(SyntaxKind::LetStmt);
        b.token(SyntaxKind::KwLet, "let".to_string());
        b.token(SyntaxKind::Whitespace, " ".to_string());
        b.token(SyntaxKind::Ident, "x".to_string());
        b.finish_node();
        b.finish_node();
        SyntaxNode::new_root(b.finish())
    }

    #[test]
    fn test_reconstruct_concatenates_leaves() {
        assert_eq!(reconstruct(&sample()), "let x");
    }

    #[test]
    fn test_spans_well_formed_on_good_tree() {
        assert_eq!(spans_well_formed(&sample(), 5), Ok(()));
    }

    #[test]
    fn test_spans_well_formed_rejects_wrong_source_len() {
        assert!(spans_well_formed(&sample(), 99).is_err());
    }

    #[test]
    fn test_every_token_present_matches_lexer() {
        let toks = axiom_lexer::lex("let x").tokens;
        assert_eq!(every_token_present(&sample(), &toks), Ok(()));
    }

    #[test]
    fn test_check_all_green_on_consistent_tree() {
        let toks = axiom_lexer::lex("let x").tokens;
        assert_eq!(check_all(&sample(), "let x", &toks), Ok(()));
    }
}
