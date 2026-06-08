//! Typed views for control-flow and composite expressions.

use super::*;

pub struct IfExpr(SyntaxNode);

impl AstNode for IfExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IfExpr
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

impl IfExpr {
    pub fn condition(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().next()
    }
    pub fn then_branch(&self) -> Option<BlockExpr> {
        child_node(&self.0)
    }
    pub fn else_branch(&self) -> Option<SyntaxNode> {
        // IfExpr children (expr-family): [condition, then_block, else_branch?]
        child_expr_nodes(&self.0).into_iter().nth(2)
    }
}

pub struct MatchExpr(SyntaxNode);

impl AstNode for MatchExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MatchExpr
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

impl MatchExpr {
    pub fn scrutinee(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
    pub fn arm_list(&self) -> Option<MatchArmList> {
        child_node(&self.0)
    }
}

pub struct LoopExpr(SyntaxNode);

impl AstNode for LoopExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LoopExpr
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

impl LoopExpr {
    pub fn label(&self) -> Option<LoopLabel> {
        child_node(&self.0)
    }
    pub fn body(&self) -> Option<BlockExpr> {
        child_node(&self.0)
    }
    /// `true` for the `loop if cond { }` form.
    pub fn is_conditional(&self) -> bool {
        child_token(&self.0, SyntaxKind::KwIf).is_some()
    }
    /// `true` for the `loop pat in iterable { }` form.
    pub fn is_iterator(&self) -> bool {
        child_token(&self.0, SyntaxKind::KwIn).is_some()
    }
    /// The condition for `loop if cond { }` — `None` for other forms.
    pub fn loop_condition(&self) -> Option<SyntaxNode> {
        if self.is_conditional() {
            child_expr_node(&self.0)
        } else {
            None
        }
    }
    /// The binding pattern for `loop pat in iterable { }` — `None` for other forms.
    pub fn iter_pattern(&self) -> Option<SyntaxNode> {
        if self.is_iterator() {
            child_pat_node(&self.0)
        } else {
            None
        }
    }
    /// The iterable expression for `loop pat in iterable { }` — `None` for other forms.
    pub fn iter_iterable(&self) -> Option<SyntaxNode> {
        if self.is_iterator() {
            child_expr_node(&self.0)
        } else {
            None
        }
    }
}

pub struct ClosureExpr(SyntaxNode);

impl AstNode for ClosureExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ClosureExpr
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

impl ClosureExpr {
    pub fn param_list(&self) -> Option<ClosureParamList> {
        child_node(&self.0)
    }
    pub fn ret_type(&self) -> Option<RetType> {
        child_node(&self.0)
    }
    pub fn body(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

pub struct StructLitExpr(SyntaxNode);

impl AstNode for StructLitExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::StructLitExpr
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

impl StructLitExpr {
    pub fn path(&self) -> Option<Path> {
        child_node(&self.0)
    }
    pub fn field_list(&self) -> Option<StructLitFieldList> {
        child_node(&self.0)
    }
}

pub struct CastExpr(SyntaxNode);

impl AstNode for CastExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CastExpr
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

impl CastExpr {
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
    pub fn ty(&self) -> Option<SyntaxNode> {
        child_type_node(&self.0)
    }
}

pub struct RangeExpr(SyntaxNode);

impl AstNode for RangeExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RangeExpr
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

impl RangeExpr {
    pub fn start(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().next()
    }
    pub fn end(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().nth(1)
    }
}

pub struct TryExpr(SyntaxNode);

impl AstNode for TryExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TryExpr
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

impl TryExpr {
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

pub struct AssignExpr(SyntaxNode);

impl AstNode for AssignExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::AssignExpr
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

impl AssignExpr {
    pub fn lhs(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().next()
    }
    pub fn rhs(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().nth(1)
    }
    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.0.children().into_iter().find_map(|e| match e {
            SyntaxElement::Token(t) if is_assign_op(t.kind()) => Some(t),
            _ => None,
        })
    }
}

fn is_assign_op(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Eq
            | SyntaxKind::PlusEq
            | SyntaxKind::MinusEq
            | SyntaxKind::StarEq
            | SyntaxKind::SlashEq
            | SyntaxKind::PercentEq
    )
}

pub struct CatchExpr(SyntaxNode);

impl AstNode for CatchExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CatchExpr
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

impl CatchExpr {
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().next()
    }
    pub fn handler(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().nth(1)
    }
}

pub struct ElseExpr(SyntaxNode);

impl AstNode for ElseExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ElseExpr
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

impl ElseExpr {
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().next()
    }
    pub fn handler(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().nth(1)
    }
}

pub struct ScopeExpr(SyntaxNode);

impl AstNode for ScopeExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ScopeExpr
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

impl ScopeExpr {
    pub fn param_list(&self) -> Option<ClosureParamList> {
        child_node(&self.0)
    }
    pub fn body(&self) -> Option<BlockExpr> {
        child_node(&self.0)
    }
}

/// Reserved for `spawn expr` (green-thread spawn, §9.3). Not yet implemented in
/// the grammar; this node kind is never produced by the current parser.
pub struct SpawnExpr(SyntaxNode);

impl AstNode for SpawnExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SpawnExpr
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

impl SpawnExpr {
    pub fn param_list(&self) -> Option<ClosureParamList> {
        child_node(&self.0)
    }
    pub fn body(&self) -> Option<BlockExpr> {
        child_node(&self.0)
    }
}

pub struct ListLitExpr(SyntaxNode);

impl AstNode for ListLitExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ListLitExpr
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

impl ListLitExpr {
    pub fn elements(&self) -> Vec<SyntaxNode> {
        child_expr_nodes(&self.0)
    }
}

pub struct ArgList(SyntaxNode);

impl AstNode for ArgList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ArgList
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

impl ArgList {
    pub fn args(&self) -> Vec<SyntaxNode> {
        child_expr_nodes(&self.0)
    }
}
