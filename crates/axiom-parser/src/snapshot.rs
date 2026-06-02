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
pub fn serialize(root: &SyntaxNode) -> String {
    let mut out = String::new();
    serialize_node(root, 0, &mut out);
    out
}

/// Emit `node`'s line, then recurse into its children one level deeper.
fn serialize_node(node: &SyntaxNode, depth: usize, out: &mut String) {
    let span = node.span();
    push_indent(out, depth);
    out.push_str(&format!(
        "{} @ {}..{}\n",
        node.kind().label(),
        span.lo,
        span.hi
    ));
    for child in node.children() {
        match child {
            SyntaxElement::Node(n) => serialize_node(&n, depth + 1, out),
            SyntaxElement::Token(t) => serialize_token(&t, depth + 1, out),
        }
    }
}

/// Emit a leaf line: `KIND @ lo..hi "repr"`.
fn serialize_token(token: &SyntaxToken, depth: usize, out: &mut String) {
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
}
