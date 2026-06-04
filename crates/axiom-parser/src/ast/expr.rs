//! Typed views for primary, operator, and postfix expressions.

use super::*;

// ── Expressions ───────────────────────────────────────────────────────────────

pub struct BlockExpr(SyntaxNode);

impl AstNode for BlockExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::BlockExpr
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

impl BlockExpr {
    pub fn stmts(&self) -> Vec<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| {
                matches!(
                    n.kind(),
                    SyntaxKind::LetStmt
                        | SyntaxKind::ExprStmt
                        | SyntaxKind::ErrdeferStmt
                        | SyntaxKind::YieldStmt
                        | SyntaxKind::Error
                )
            })
            .collect()
    }
}

pub struct LiteralExpr(SyntaxNode);

impl AstNode for LiteralExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LiteralExpr
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

impl LiteralExpr {
    pub fn token(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }
}

pub struct PathExpr(SyntaxNode);

impl AstNode for PathExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PathExpr
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

impl PathExpr {
    pub fn path(&self) -> Option<Path> {
        child_node(&self.0)
    }
}

pub struct BinExpr(SyntaxNode);

impl AstNode for BinExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::BinExpr
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

impl BinExpr {
    pub fn lhs(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().next()
    }
    pub fn rhs(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().nth(1)
    }
    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.0.children().into_iter().find_map(|e| match e {
            SyntaxElement::Token(t) if is_bin_op(t.kind()) => Some(t),
            _ => None,
        })
    }
}

fn is_bin_op(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Plus
            | SyntaxKind::Minus
            | SyntaxKind::Star
            | SyntaxKind::Slash
            | SyntaxKind::Percent
            | SyntaxKind::Amp
            | SyntaxKind::AmpAmp
            | SyntaxKind::Pipe
            | SyntaxKind::PipePipe
            | SyntaxKind::Caret
            | SyntaxKind::Shl
            | SyntaxKind::Shr
            | SyntaxKind::EqEq
            | SyntaxKind::Ne
            | SyntaxKind::Lt
            | SyntaxKind::Le
            | SyntaxKind::Gt
            | SyntaxKind::Ge
    )
}

pub struct PrefixExpr(SyntaxNode);

impl AstNode for PrefixExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PrefixExpr
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

impl PrefixExpr {
    pub fn op_token(&self) -> Option<SyntaxToken> {
        self.0.children().into_iter().find_map(|e| match e {
            SyntaxElement::Token(t) if matches!(t.kind(), SyntaxKind::Minus | SyntaxKind::Bang) => {
                Some(t)
            }
            _ => None,
        })
    }
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

pub struct CallExpr(SyntaxNode);

impl AstNode for CallExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CallExpr
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

impl CallExpr {
    pub fn callee(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        child_node(&self.0)
    }
}

pub struct MethodCallExpr(SyntaxNode);

impl AstNode for MethodCallExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MethodCallExpr
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

impl MethodCallExpr {
    pub fn receiver(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
    pub fn method_name(&self) -> Option<NameRef> {
        child_node(&self.0)
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        child_node(&self.0)
    }
}

pub struct FieldExpr(SyntaxNode);

impl AstNode for FieldExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldExpr
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

impl FieldExpr {
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
    pub fn field_name(&self) -> Option<NameRef> {
        child_node(&self.0)
    }
}

pub struct IndexExpr(SyntaxNode);

impl AstNode for IndexExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IndexExpr
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

impl IndexExpr {
    pub fn base(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().next()
    }
    pub fn index(&self) -> Option<SyntaxNode> {
        child_expr_nodes(&self.0).into_iter().nth(1)
    }
}

pub struct ParenExpr(SyntaxNode);

impl AstNode for ParenExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ParenExpr
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

impl ParenExpr {
    pub fn expr(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}
