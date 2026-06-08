//! Typed views for loop headers, match arms, closure params, and struct-literal fields.

use super::*;

// ── Loop forms ────────────────────────────────────────────────────────────────

/// Reserved for a future grammar refactor that wraps the `loop if` condition.
/// The current grammar emits the condition directly into `LoopExpr`; this node
/// kind is never produced by the parser today. Use `LoopExpr::loop_condition()`.
pub struct LoopCondition(SyntaxNode);

impl AstNode for LoopCondition {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LoopCondition
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

impl LoopCondition {
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

/// Reserved for a future grammar refactor that wraps the `loop pat in` iterator
/// header. The current grammar emits the pattern and iterable directly into
/// `LoopExpr`; this node kind is never produced today. Use
/// `LoopExpr::iter_pattern()` / `LoopExpr::iter_iterable()`.
pub struct LoopIter(SyntaxNode);

impl AstNode for LoopIter {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LoopIter
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

impl LoopIter {
    pub fn pattern(&self) -> Option<SyntaxNode> {
        child_pat_node(&self.0)
    }
    pub fn iterable(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

pub struct LoopLabel(SyntaxNode);

impl AstNode for LoopLabel {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LoopLabel
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

impl LoopLabel {
    pub fn label_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Label)
    }
}

// ── Match ─────────────────────────────────────────────────────────────────────

pub struct MatchArmList(SyntaxNode);

impl AstNode for MatchArmList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MatchArmList
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

impl MatchArmList {
    pub fn arms(&self) -> Vec<MatchArm> {
        child_nodes_of(&self.0)
    }
}

pub struct MatchArm(SyntaxNode);

impl AstNode for MatchArm {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MatchArm
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

impl MatchArm {
    pub fn pattern(&self) -> Option<SyntaxNode> {
        child_pat_node(&self.0)
    }
    pub fn guard(&self) -> Option<MatchGuard> {
        child_node(&self.0)
    }
    pub fn body(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

pub struct MatchGuard(SyntaxNode);

impl AstNode for MatchGuard {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MatchGuard
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

impl MatchGuard {
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

// ── Closures + struct literals ────────────────────────────────────────────────

pub struct ClosureParamList(SyntaxNode);

impl AstNode for ClosureParamList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ClosureParamList
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

impl ClosureParamList {
    pub fn params(&self) -> Vec<ClosureParam> {
        child_nodes_of(&self.0)
    }
}

pub struct ClosureParam(SyntaxNode);

impl AstNode for ClosureParam {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ClosureParam
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

impl ClosureParam {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Ident)
    }
    pub fn ty(&self) -> Option<SyntaxNode> {
        child_type_node(&self.0)
    }
}

pub struct StructLitFieldList(SyntaxNode);

impl AstNode for StructLitFieldList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::StructLitFieldList
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

impl StructLitFieldList {
    pub fn fields(&self) -> Vec<StructLitField> {
        child_nodes_of(&self.0)
    }
    /// The `..base` spread expression, if present. The grammar emits the spread
    /// directly into this node (no wrapping child node), so `fields()` does not
    /// include it.
    pub fn spread_base(&self) -> Option<SyntaxNode> {
        if child_token(&self.0, SyntaxKind::DotDot).is_some() {
            child_expr_node(&self.0)
        } else {
            None
        }
    }
}

pub struct StructLitField(SyntaxNode);

impl AstNode for StructLitField {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::StructLitField
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

impl StructLitField {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Ident)
    }
    pub fn value(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}
