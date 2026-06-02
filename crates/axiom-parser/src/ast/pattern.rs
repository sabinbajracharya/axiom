//! Typed views for patterns.

use super::*;

// ── Patterns ──────────────────────────────────────────────────────────────────

pub struct WildcardPat(SyntaxNode);

impl AstNode for WildcardPat {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::WildcardPat
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

pub struct LiteralPat(SyntaxNode);

impl AstNode for LiteralPat {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralPat
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

impl LiteralPat {
    pub fn token(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }
}

pub struct IdentPat(SyntaxNode);

impl AstNode for IdentPat {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IdentPat
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

impl IdentPat {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }
}

pub struct TupleStructPat(SyntaxNode);

impl AstNode for TupleStructPat {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TupleStructPat
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

impl TupleStructPat {
    pub fn path(&self) -> Option<Path> {
        child_node(&self.0)
    }
    pub fn fields(&self) -> Option<TuplePatFieldList> {
        child_node(&self.0)
    }
}

pub struct StructPat(SyntaxNode);

impl AstNode for StructPat {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::StructPat
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

impl StructPat {
    pub fn path(&self) -> Option<Path> {
        child_node(&self.0)
    }
    pub fn field_list(&self) -> Option<StructPatFieldList> {
        child_node(&self.0)
    }
}

pub struct PathPat(SyntaxNode);

impl AstNode for PathPat {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PathPat
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

impl PathPat {
    pub fn path(&self) -> Option<Path> {
        child_node(&self.0)
    }
}

pub struct OrPat(SyntaxNode);

impl AstNode for OrPat {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::OrPat
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

impl OrPat {
    pub fn alternatives(&self) -> Vec<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| is_pat(n.kind()))
            .collect()
    }
}

pub struct RestPat(SyntaxNode);

impl AstNode for RestPat {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RestPat
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

pub struct RangePat(SyntaxNode);

impl AstNode for RangePat {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RangePat
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

impl RangePat {
    /// The `..` or `..=` range operator token.
    pub fn range_op_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::DotDot)
            .or_else(|| child_token(&self.0, SyntaxKind::DotDotEq))
    }
    /// The start bound's literal token. For negative bounds (e.g. `-1..=9`),
    /// this skips the leading `-` and returns the literal itself.
    pub fn start_literal(&self) -> Option<SyntaxToken> {
        self.0.children().into_iter().find_map(|e| match e {
            SyntaxElement::Token(t) if is_range_pat_literal(t.kind()) => Some(t),
            _ => None,
        })
    }
    /// The end bound's literal token. For negative bounds, this skips the `-`.
    pub fn end_literal(&self) -> Option<SyntaxToken> {
        self.0
            .children()
            .into_iter()
            .filter_map(|e| match e {
                SyntaxElement::Token(t) if is_range_pat_literal(t.kind()) => Some(t),
                _ => None,
            })
            .nth(1)
    }
}

fn is_range_pat_literal(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::IntLit
            | SyntaxKind::FloatLit
            | SyntaxKind::ByteLit
            | SyntaxKind::StrLit
            | SyntaxKind::KwTrue
            | SyntaxKind::KwFalse
    )
}

pub struct StructPatFieldList(SyntaxNode);

impl AstNode for StructPatFieldList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::StructPatFieldList
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

impl StructPatFieldList {
    pub fn fields(&self) -> Vec<StructPatField> {
        child_nodes_of(&self.0)
    }
}

pub struct StructPatField(SyntaxNode);

impl AstNode for StructPatField {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::StructPatField
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

impl StructPatField {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Ident)
    }
    pub fn pattern(&self) -> Option<SyntaxNode> {
        child_pat_node(&self.0)
    }
}

pub struct TuplePatFieldList(SyntaxNode);

impl AstNode for TuplePatFieldList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TuplePatFieldList
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

impl TuplePatFieldList {
    pub fn patterns(&self) -> Vec<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| is_pat(n.kind()))
            .collect()
    }
}
