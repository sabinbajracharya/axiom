//! Typed views for paths and names.

use super::*;

// ── Paths + names ─────────────────────────────────────────────────────────────

pub struct Path(SyntaxNode);

impl AstNode for Path {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Path
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl Path {
    pub fn segments(&self) -> Vec<PathSegment> {
        child_nodes_of(&self.0)
    }
}

pub struct PathSegment(SyntaxNode);

impl AstNode for PathSegment {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PathSegment
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl PathSegment {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }
}

pub struct Name(SyntaxNode);

impl AstNode for Name {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Name
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl Name {
    pub fn ident_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Ident)
    }
    pub fn text(&self) -> Option<String> {
        self.ident_token().map(|t| t.text().to_string())
    }
}

pub struct NameRef(SyntaxNode);

impl AstNode for NameRef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::NameRef
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl NameRef {
    /// The identifier or keyword token used as a name.
    pub fn token(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }
    pub fn text(&self) -> Option<String> {
        self.token().map(|t| t.text().to_string())
    }
}
