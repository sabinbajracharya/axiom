//! The **green tree** (`docs/parser-testing.md` §2.2): immutable,
//! position-independent nodes and tokens shared via `Rc`, built bottom-up by
//! `GreenNodeBuilder`. A green node knows its text *length*, never its *offset*
//! — offsets are a red-tree concern (`syntax.rs`), so identical subtrees stay
//! shareable. Nothing here mutates after construction.

use crate::syntax_kind::SyntaxKind;
use std::rc::Rc;

/// A leaf: a kind plus the exact source text it covers. Storing the text is what
/// keeps the tree losslessly reconstructable (`invariants::reconstruct`).
#[derive(Debug, PartialEq)]
pub struct GreenToken {
    pub kind: SyntaxKind,
    pub text: String,
}

impl GreenToken {
    pub fn text_len(&self) -> usize {
        self.text.len()
    }
}

/// An interior node: a kind, its total text length (sum of children), and its
/// ordered children. `text_len` is cached so red-tree offset math is O(1).
#[derive(Debug, PartialEq)]
pub struct GreenNode {
    pub kind: SyntaxKind,
    text_len: usize,
    pub children: Vec<GreenChild>,
}

impl GreenNode {
    pub fn text_len(&self) -> usize {
        self.text_len
    }
}

/// Dismantle a green node **iteratively** so dropping a pathologically deep tree
/// (a long operator chain, a deeply nested literal, a deep recovery subtree)
/// cannot overflow the stack. The naive recursive drop would recurse once per
/// level; instead we hoist uniquely-owned descendants onto a work stack and let
/// each node drop with already-emptied children (no further recursion). Shared
/// subtrees (refcount > 1) are left alone — only their count is decremented.
impl Drop for GreenNode {
    fn drop(&mut self) {
        let mut stack: Vec<GreenChild> = std::mem::take(&mut self.children);
        while let Some(child) = stack.pop() {
            if let GreenChild::Node(rc) = child {
                if let Ok(mut node) = Rc::try_unwrap(rc) {
                    stack.extend(std::mem::take(&mut node.children));
                }
            }
        }
    }
}

/// A child slot in a green node: either a subtree or a leaf.
#[derive(Debug, Clone, PartialEq)]
pub enum GreenChild {
    Node(Rc<GreenNode>),
    Token(Rc<GreenToken>),
}

impl GreenChild {
    pub fn text_len(&self) -> usize {
        match self {
            GreenChild::Node(n) => n.text_len(),
            GreenChild::Token(t) => t.text_len(),
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        match self {
            GreenChild::Node(n) => n.kind,
            GreenChild::Token(t) => t.kind,
        }
    }
}

/// Builds a green tree bottom-up with a `start_node` / `token` / `finish_node`
/// discipline. One stateful core (§8.1); driven by `event::build_tree`, never by
/// the grammar directly.
pub struct GreenNodeBuilder {
    /// Open nodes: each holds its kind and the children accumulated so far. The
    /// last entry is the currently-open node.
    stack: Vec<(SyntaxKind, Vec<GreenChild>)>,
    /// The completed root, set when the outermost `finish_node` pops the stack
    /// empty.
    root: Option<Rc<GreenNode>>,
}

impl GreenNodeBuilder {
    pub fn new() -> GreenNodeBuilder {
        GreenNodeBuilder {
            stack: Vec::new(),
            root: None,
        }
    }

    /// Open a new interior node; subsequent tokens/nodes nest inside it until the
    /// matching `finish_node`.
    pub fn start_node(&mut self, kind: SyntaxKind) {
        self.stack.push((kind, Vec::new()));
    }

    /// Append a leaf to the currently-open node. (The builder is always driven
    /// with a root open before any token, so the stack is never empty here.)
    pub fn token(&mut self, kind: SyntaxKind, text: String) {
        let leaf = GreenChild::Token(Rc::new(GreenToken { kind, text }));
        if let Some((_, children)) = self.stack.last_mut() {
            children.push(leaf);
        }
    }

    /// Close the currently-open node, attaching it to its parent — or recording
    /// it as the root if the stack is now empty.
    pub fn finish_node(&mut self) {
        let Some((kind, children)) = self.stack.pop() else {
            return;
        };
        let text_len = children.iter().map(GreenChild::text_len).sum();
        let node = Rc::new(GreenNode {
            kind,
            text_len,
            children,
        });
        match self.stack.last_mut() {
            Some((_, parent)) => parent.push(GreenChild::Node(node)),
            None => self.root = Some(node),
        }
    }

    /// Consume the builder, returning the root. An empty/never-started build
    /// yields an empty `SourceFile` so callers never face an `Option` panic.
    pub fn finish(self) -> Rc<GreenNode> {
        self.root.unwrap_or_else(|| {
            Rc::new(GreenNode {
                kind: SyntaxKind::SourceFile,
                text_len: 0,
                children: Vec::new(),
            })
        })
    }
}

impl Default for GreenNodeBuilder {
    fn default() -> GreenNodeBuilder {
        GreenNodeBuilder::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builds_nested_tree_with_lengths() {
        // SourceFile { LetStmt { KwLet "let" Whitespace " " Ident "x" } }
        let mut b = GreenNodeBuilder::new();
        b.start_node(SyntaxKind::SourceFile);
        b.start_node(SyntaxKind::LetStmt);
        b.token(SyntaxKind::KwLet, "let".to_string());
        b.token(SyntaxKind::Whitespace, " ".to_string());
        b.token(SyntaxKind::Ident, "x".to_string());
        b.finish_node();
        b.finish_node();
        let root = b.finish();

        assert_eq!(root.kind, SyntaxKind::SourceFile);
        assert_eq!(root.text_len(), 5); // "let" + " " + "x"
        assert_eq!(root.children.len(), 1);
        let GreenChild::Node(n) = &root.children[0] else {
            unreachable!("expected a node child");
        };
        assert_eq!(n.kind, SyntaxKind::LetStmt);
        assert_eq!(n.children.len(), 3);
        assert_eq!(n.text_len(), 5);
    }

    #[test]
    fn test_empty_build_yields_empty_source_file() {
        let root = GreenNodeBuilder::new().finish();
        assert_eq!(root.kind, SyntaxKind::SourceFile);
        assert_eq!(root.text_len(), 0);
    }

    #[test]
    fn test_deep_tree_drops_without_stack_overflow() {
        // A degenerate, very deep tree. Building is iterative; the iterative
        // `Drop` must dismantle it without recursing per level — a naive
        // recursive drop would overflow the test thread's stack here.
        let depth = 200_000;
        let mut b = GreenNodeBuilder::new();
        for _ in 0..depth {
            b.start_node(SyntaxKind::BlockExpr);
        }
        b.token(SyntaxKind::Ident, "x".to_string());
        for _ in 0..depth {
            b.finish_node();
        }
        let root = b.finish();
        drop(root); // must return without overflowing
    }
}
