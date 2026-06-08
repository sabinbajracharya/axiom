//! Coverage invariants (`docs/parser-testing.md` §4) — the parser's mechanical
//! "nothing was missed" proof, defined **once** and called by golden tests, unit
//! tests, and the fuzzer alike (DRY: never re-implemented).
//!
//! - `reconstruct` — the tree rebuilds the source byte-for-byte (losslessness).
//! - `spans_well_formed` — the tree *shape* is sound (nesting + contiguity).
//! - `every_token_present` — every significant lexer token landed in the tree,
//!   in order, exactly once, with the right kind (split-aware).
//!
//! All pure; they take a tree and return data or a typed error, never panic.

use crate::syntax::{SyntaxElement, SyntaxNode, SyntaxToken};
use crate::syntax_kind::SyntaxKind;
use lexer::Token;

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
    /// The significant (non-trivia) text in the tree did not match the lexer's
    /// — a byte was dropped, duplicated, reordered, or misclassified as trivia.
    SignificantTextMismatch { tree: String, lexer: String },
    /// A lexer token's bytes were covered by tree leaves of the wrong kind. The
    /// expected kind is the lexer token's `from_lexer` kind; the found kinds are
    /// the leaf (or split-leaf) kinds covering those bytes. Catches a token
    /// silently mis-kinded — including a wrong **split** (`>>`→`Gt Lt`).
    KindMismatch {
        lexer: SyntaxKind,
        tree: Vec<SyntaxKind>,
    },
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

/// Per-node shape check (each node's direct children must nest inside and tile
/// it), applied to every node in the tree.
///
/// Iterative (explicit work-stack) rather than recursive, so a pathologically
/// deep tree cannot overflow the stack during checking.
fn check_node(root: &SyntaxNode) -> Result<(), SpanError> {
    let mut stack: Vec<SyntaxNode> = vec![root.clone()];
    while let Some(node) = stack.pop() {
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
        // unless the node is a childless leaf-holder (an empty node where
        // cursor stayed at lo == hi). A node with children must tile exactly.
        if !children.is_empty() && cursor != parent.hi {
            return Err(SpanError::NodeSpanMismatch { node: node.kind() });
        }
        for child in children {
            if let SyntaxElement::Node(n) = child {
                stack.push(n);
            }
        }
    }
    Ok(())
}

/// Assert every significant (non-trivia) lexer token is present in the tree, in
/// order, with the right kind. Proves no significant byte was dropped,
/// duplicated, reordered, or misclassified as trivia **and** that no leaf was
/// mis-kinded.
///
/// The parser may split one source token into several leaves (`>>` → `Gt` `Gt`
/// to close nested generics), so the tree's leaves are 1-to-many against the
/// lexer's tokens. We therefore walk the two streams by **byte coverage**: each
/// lexer token is covered by either one leaf of the same `from_lexer` kind, or
/// by the exact leaf sequence of a sanctioned split. A wrong kind on a whole
/// token *or* on a split half fails. `Eof` and trivia are excluded on both
/// sides.
pub fn every_token_present(root: &SyntaxNode, lexer_tokens: &[Token]) -> Result<(), CoverageError> {
    let leaves: Vec<SyntaxToken> = root
        .tokens()
        .into_iter()
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let lex_sig: Vec<&Token> = lexer_tokens
        .iter()
        .filter(|t| !t.kind.is_trivia() && t.kind != lexer::TokenKind::Eof)
        .collect();

    // Byte-level coverage first: if the significant text differs, report that
    // (the stronger, kind-aware walk below assumes the bytes line up).
    let tree_text: String = leaves.iter().map(|t| t.text()).collect();
    let lexer_text: String = lex_sig.iter().map(|t| t.text.as_str()).collect();
    if tree_text != lexer_text {
        return Err(CoverageError::SignificantTextMismatch {
            tree: tree_text,
            lexer: lexer_text,
        });
    }

    // Kind-aware walk: consume leaves until their bytes cover each lexer token,
    // then validate the covering kinds. Because splits only subdivide a single
    // token (never merge across tokens) and the text already matches, the
    // accumulated bytes always land exactly on each token boundary.
    let mut next = 0usize;
    for lt in &lex_sig {
        let expected = SyntaxKind::from_lexer(&lt.kind);
        let mut covered = 0usize;
        let mut kinds: Vec<SyntaxKind> = Vec::new();
        while covered < lt.text.len() {
            let Some(leaf) = leaves.get(next) else {
                return Err(CoverageError::SignificantTextMismatch {
                    tree: tree_text,
                    lexer: lexer_text,
                });
            };
            covered += leaf.text().len();
            kinds.push(leaf.kind());
            next += 1;
        }
        let ok = match kinds.as_slice() {
            [only] => *only == expected,
            split => split_pieces(expected).is_some_and(|pieces| pieces == split),
        };
        if !ok {
            return Err(CoverageError::KindMismatch {
                lexer: expected,
                tree: kinds,
            });
        }
    }
    Ok(())
}

/// The leaf kinds a compound token legitimately splits into. The parser splits a
/// leading `>` off `>>`/`>=` to close nested generics (`Parser::split_one_gt`);
/// these are the only sanctioned 1-to-many leaf coverings.
fn split_pieces(kind: SyntaxKind) -> Option<&'static [SyntaxKind]> {
    match kind {
        SyntaxKind::Shr => Some(&[SyntaxKind::Gt, SyntaxKind::Gt]),
        SyntaxKind::Ge => Some(&[SyntaxKind::Gt, SyntaxKind::Eq]),
        _ => None,
    }
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
        let toks = lexer::lex("let x").tokens;
        assert_eq!(every_token_present(&sample(), &toks), Ok(()));
    }

    #[test]
    fn test_check_all_green_on_consistent_tree() {
        let toks = lexer::lex("let x").tokens;
        assert_eq!(check_all(&sample(), "let x", &toks), Ok(()));
    }

    /// A leaf with the right text but the wrong kind must be caught (the Gap 2
    /// hardening: the check is kind-aware, not just text-aware).
    #[test]
    fn test_every_token_present_rejects_miskinded_leaf() {
        let mut b = GreenNodeBuilder::new();
        b.start_node(SyntaxKind::SourceFile);
        b.token(SyntaxKind::IntLit, "x".to_string()); // wrong kind, right text
        b.finish_node();
        let root = SyntaxNode::new_root(b.finish());
        let toks = lexer::lex("x").tokens; // real kind: Ident
        assert_eq!(
            every_token_present(&root, &toks),
            Err(CoverageError::KindMismatch {
                lexer: SyntaxKind::Ident,
                tree: vec![SyntaxKind::IntLit],
            })
        );
    }

    /// A sanctioned split (`>>` → `Gt` `Gt`, as the parser emits to close nested
    /// generics) must pass; a wrong split must fail.
    #[test]
    fn test_every_token_present_handles_token_split() {
        let toks = lexer::lex(">>").tokens; // one Shr token

        let mut good = GreenNodeBuilder::new();
        good.start_node(SyntaxKind::SourceFile);
        good.token(SyntaxKind::Gt, ">".to_string());
        good.token(SyntaxKind::Gt, ">".to_string());
        good.finish_node();
        let good = SyntaxNode::new_root(good.finish());
        assert_eq!(every_token_present(&good, &toks), Ok(()));

        let mut bad = GreenNodeBuilder::new();
        bad.start_node(SyntaxKind::SourceFile);
        bad.token(SyntaxKind::Gt, ">".to_string());
        bad.token(SyntaxKind::Lt, ">".to_string()); // wrong split half
        bad.finish_node();
        let bad = SyntaxNode::new_root(bad.finish());
        assert!(matches!(
            every_token_present(&bad, &toks),
            Err(CoverageError::KindMismatch { .. })
        ));
    }

    /// A deep left-leaning tree (depth = chain length) must not overflow the
    /// stack in the linear consumers — `reconstruct` and `spans_well_formed` —
    /// nor when the red nodes are dropped (the Gap 1 fix: traversal *and* the
    /// red-node parent-chain `Drop` are iterative now). A depth this large
    /// overflowed the previous recursive implementations.
    #[test]
    fn test_deep_tree_does_not_overflow_consumers() {
        let mut b = GreenNodeBuilder::new();
        let depth = 60_000;
        b.start_node(SyntaxKind::SourceFile);
        for _ in 0..depth {
            b.start_node(SyntaxKind::BinExpr);
        }
        b.token(SyntaxKind::IntLit, "1".to_string());
        for _ in 0..depth {
            b.finish_node();
        }
        b.finish_node();
        let root = SyntaxNode::new_root(b.finish());
        assert_eq!(reconstruct(&root), "1");
        assert_eq!(spans_well_formed(&root, 1), Ok(()));
        // `serialize` shares the same iterative work-stack; its output is
        // inherently O(depth^2) (indentation = depth), so it is exercised for
        // non-overflow at a smaller depth in `snapshot::tests`.
    }
}
