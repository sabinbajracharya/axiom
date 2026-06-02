//! Canonical tree snapshot serializer (`docs/parser-testing.md` §6). Pure
//! functions: `&SyntaxNode → String`. This output is both the debug dump
//! (`examples/parse.rs`) and the golden-test oracle, so the format is a
//! contract: LF-only, deterministic, indentation = depth.
//!
//! Kind names come exclusively from `SyntaxKind::label` — never spelled here
//! (enforced by `test_no_hardcoded_kind_labels`). Only escape sequences and
//! format punctuation are literals.

use crate::syntax::{SyntaxElement, SyntaxNode, SyntaxToken};

const INDENT: &str = "  ";

/// Serialize a tree to the canonical indented form: one node/token per line, two
/// spaces per depth level.
///
/// Iterative (explicit work-stack of `(element, depth)`) rather than recursive,
/// so a pathologically deep tree — e.g. a long left-associative operator chain —
/// cannot overflow the stack while being dumped.
pub fn serialize(root: &SyntaxNode) -> String {
    let mut out = String::new();
    let mut stack: Vec<(SyntaxElement, usize)> = vec![(SyntaxElement::Node(root.clone()), 0)];
    while let Some((element, depth)) = stack.pop() {
        match element {
            SyntaxElement::Node(node) => {
                emit_node_line(&node, depth, &mut out);
                // Push children reversed so the leftmost is popped (emitted) first.
                for child in node.children().into_iter().rev() {
                    stack.push((child, depth + 1));
                }
            }
            SyntaxElement::Token(token) => emit_token_line(&token, depth, &mut out),
        }
    }
    out
}

/// Emit `node`'s line: `KIND @ lo..hi`.
fn emit_node_line(node: &SyntaxNode, depth: usize, out: &mut String) {
    let span = node.span();
    push_indent(out, depth);
    out.push_str(&format!(
        "{} @ {}..{}\n",
        node.kind().label(),
        span.lo,
        span.hi
    ));
}

/// Emit a leaf line: `KIND @ lo..hi "repr"`.
fn emit_token_line(token: &SyntaxToken, depth: usize, out: &mut String) {
    let span = token.span();
    push_indent(out, depth);
    out.push_str(&format!(
        "{} @ {}..{} {}\n",
        token.kind().label(),
        span.lo,
        span.hi,
        quote(token.text()),
    ));
}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str(INDENT);
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", escape(s))
}

/// Escape so a token always occupies exactly one snapshot line (same rules as
/// the lexer dump).
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::green::GreenNodeBuilder;
    use crate::syntax_kind::SyntaxKind;

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
    fn test_serialize_indented_tree() {
        let out = serialize(&sample());
        assert_eq!(
            out,
            "SourceFile @ 0..5\n  \
             LetStmt @ 0..5\n    \
             KwLet @ 0..3 \"let\"\n    \
             Whitespace @ 3..4 \" \"\n    \
             Ident @ 4..5 \"x\"\n"
        );
    }

    #[test]
    fn test_no_hardcoded_kind_labels() {
        // The serializer must not spell any SyntaxKind label as a literal — they
        // all come from SyntaxKind::label. Scan this file's own source.
        let src = include_str!("snapshot.rs");
        for kind in SyntaxKind::ALL {
            let needle = format!("\"{}\"", kind.label());
            assert!(
                !src.contains(&needle),
                "snapshot.rs hardcodes kind label {needle}; use SyntaxKind::label"
            );
        }
    }

    /// A deep tree must serialize via the iterative work-stack without
    /// overflowing the stack (Gap 1). Depth is kept moderate because the output
    /// is inherently O(depth^2) (indentation = depth); the linear consumers are
    /// proven at far higher depth in `invariants::tests`.
    #[test]
    fn test_serialize_deep_tree_does_not_overflow() {
        let mut b = GreenNodeBuilder::new();
        let depth = 8_000;
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
        assert!(serialize(&root).contains(SyntaxKind::IntLit.label()));
    }
}
