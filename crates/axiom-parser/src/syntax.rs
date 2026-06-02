//! The **red tree** (`docs/parser-testing.md` §2.3): a navigable wrapper over
//! the green tree that adds **absolute offsets** and **parent pointers**,
//! computed lazily on traversal. This is the layer the snapshot serializer, the
//! coverage invariants, and the typed AST views walk.
//!
//! Offsets are *derived* by accumulating green child lengths, so byte offset
//! stays the single positional truth (same discipline as the lexer's `Span`).

use crate::green::{GreenChild, GreenNode, GreenToken};
use crate::syntax_kind::SyntaxKind;
use axiom_lexer::Span;
use std::rc::Rc;

/// A node in the red tree: a shared green node plus where it sits in the source
/// and who its parent is. Cheap to clone (`Rc` + a `usize`).
#[derive(Clone)]
pub struct SyntaxNode {
    green: Rc<GreenNode>,
    offset: usize,
    parent: Option<Rc<SyntaxNode>>,
}

/// A leaf in the red tree: a shared green token plus its absolute position.
#[derive(Clone)]
pub struct SyntaxToken {
    green: Rc<GreenToken>,
    offset: usize,
    parent: Option<Rc<SyntaxNode>>,
}

/// Either kind of red-tree child.
#[derive(Clone)]
pub enum SyntaxElement {
    Node(SyntaxNode),
    Token(SyntaxToken),
}

impl SyntaxNode {
    /// Wrap a green root as the red root (offset 0, no parent).
    pub fn new_root(green: Rc<GreenNode>) -> SyntaxNode {
        SyntaxNode {
            green,
            offset: 0,
            parent: None,
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.green.kind
    }

    /// Half-open byte span `[offset, offset + len)`.
    pub fn span(&self) -> Span {
        Span {
            lo: self.offset,
            hi: self.offset + self.green.text_len(),
        }
    }

    pub fn parent(&self) -> Option<&SyntaxNode> {
        self.parent.as_deref()
    }

    /// The node's children (nodes and tokens), each tagged with its absolute
    /// offset, in source order.
    pub fn children(&self) -> Vec<SyntaxElement> {
        let me = Rc::new(self.clone());
        let mut offset = self.offset;
        let mut out = Vec::with_capacity(self.green.children.len());
        for child in &self.green.children {
            out.push(red_child(child, offset, &me));
            offset += child.text_len();
        }
        out
    }

    /// Child nodes only (typed-view navigation skips leaves).
    pub fn child_nodes(&self) -> Vec<SyntaxNode> {
        self.children()
            .into_iter()
            .filter_map(|e| match e {
                SyntaxElement::Node(n) => Some(n),
                SyntaxElement::Token(_) => None,
            })
            .collect()
    }

    /// All leaf tokens under this node, in left-to-right order. The basis of
    /// `invariants::reconstruct`.
    pub fn tokens(&self) -> Vec<SyntaxToken> {
        let mut out = Vec::new();
        collect_tokens(self, &mut out);
        out
    }
}

impl SyntaxToken {
    pub fn kind(&self) -> SyntaxKind {
        self.green.kind
    }

    pub fn text(&self) -> &str {
        &self.green.text
    }

    pub fn span(&self) -> Span {
        Span {
            lo: self.offset,
            hi: self.offset + self.green.text_len(),
        }
    }

    pub fn parent(&self) -> Option<&SyntaxNode> {
        self.parent.as_deref()
    }
}

impl SyntaxElement {
    pub fn kind(&self) -> SyntaxKind {
        match self {
            SyntaxElement::Node(n) => n.kind(),
            SyntaxElement::Token(t) => t.kind(),
        }
    }

    pub fn span(&self) -> Span {
        match self {
            SyntaxElement::Node(n) => n.span(),
            SyntaxElement::Token(t) => t.span(),
        }
    }
}

/// Build the red child sitting at `offset` under `parent`.
fn red_child(child: &GreenChild, offset: usize, parent: &Rc<SyntaxNode>) -> SyntaxElement {
    match child {
        GreenChild::Node(n) => SyntaxElement::Node(SyntaxNode {
            green: n.clone(),
            offset,
            parent: Some(parent.clone()),
        }),
        GreenChild::Token(t) => SyntaxElement::Token(SyntaxToken {
            green: t.clone(),
            offset,
            parent: Some(parent.clone()),
        }),
    }
}

/// Depth-first collect of every leaf token under `node`.
fn collect_tokens(node: &SyntaxNode, out: &mut Vec<SyntaxToken>) {
    for child in node.children() {
        match child {
            SyntaxElement::Node(n) => collect_tokens(&n, out),
            SyntaxElement::Token(t) => out.push(t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::green::GreenNodeBuilder;

    fn sample() -> SyntaxNode {
        // SourceFile { LetStmt { "let" " " "x" } }
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
    fn test_root_span_covers_whole_source() {
        let root = sample();
        assert_eq!(root.span(), Span { lo: 0, hi: 5 });
    }

    #[test]
    fn test_child_offsets_accumulate() {
        let root = sample();
        let stmt = &root.child_nodes()[0];
        let toks = stmt.children();
        assert_eq!(toks[0].span(), Span { lo: 0, hi: 3 }); // "let"
        assert_eq!(toks[1].span(), Span { lo: 3, hi: 4 }); // " "
        assert_eq!(toks[2].span(), Span { lo: 4, hi: 5 }); // "x"
    }

    #[test]
    fn test_tokens_flattens_in_source_order() {
        let root = sample();
        let texts: Vec<String> = root.tokens().iter().map(|t| t.text().to_string()).collect();
        assert_eq!(texts, vec!["let", " ", "x"]);
    }

    #[test]
    fn test_parent_pointer_links_up() {
        let root = sample();
        let stmt = &root.child_nodes()[0];
        assert_eq!(
            stmt.parent().map(SyntaxNode::kind),
            Some(SyntaxKind::SourceFile)
        );
    }
}
