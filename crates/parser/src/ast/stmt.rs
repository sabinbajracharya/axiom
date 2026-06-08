//! Typed views for statements (let / expr / return / break / continue / errdefer).

use super::*;

// ── Statements ────────────────────────────────────────────────────────────────

pub struct LetStmt(SyntaxNode);

impl AstNode for LetStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LetStmt
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

impl LetStmt {
    pub fn binding_kw(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::KwVal).or_else(|| child_token(&self.0, SyntaxKind::KwVar))
    }
    pub fn pattern(&self) -> Option<SyntaxNode> {
        child_pat_node(&self.0)
    }
    pub fn ty(&self) -> Option<SyntaxNode> {
        child_type_node(&self.0)
    }
    pub fn value(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

pub struct ExprStmt(SyntaxNode);

impl AstNode for ExprStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ExprStmt
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

impl ExprStmt {
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }

    /// Whether this statement is terminated by a `;`. A block's final expression
    /// is its value only when there is *no* trailing semicolon (DESIGN_SPEC §16);
    /// a trailing `;` discards the value, making the block evaluate to `()`.
    pub fn has_semicolon(&self) -> bool {
        child_token(&self.0, SyntaxKind::Semicolon).is_some()
    }
}

pub struct ReturnStmt(SyntaxNode);

impl AstNode for ReturnStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ReturnStmt
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

impl ReturnStmt {
    pub fn value(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

pub struct BreakStmt(SyntaxNode);

impl AstNode for BreakStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::BreakStmt
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

impl BreakStmt {
    pub fn label_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Label)
    }
    pub fn value(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

pub struct ContinueStmt(SyntaxNode);

impl AstNode for ContinueStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ContinueStmt
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

impl ContinueStmt {
    pub fn label_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Label)
    }
}

pub struct ErrdeferStmt(SyntaxNode);

impl AstNode for ErrdeferStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ErrdeferStmt
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

impl ErrdeferStmt {
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

pub struct YieldStmt(SyntaxNode);

impl AstNode for YieldStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::YieldStmt
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

impl YieldStmt {
    pub fn value(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}
