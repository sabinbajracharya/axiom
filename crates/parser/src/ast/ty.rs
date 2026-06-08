//! Typed views for type annotations.

use super::*;

// ── Types ─────────────────────────────────────────────────────────────────────

pub struct PathType(SyntaxNode);

impl AstNode for PathType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PathType
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

impl PathType {
    /// The path for a named type like `Foo` or `pkg::Bar`.
    ///
    /// Returns `None` for parenthesized types like `(T)`: the grammar reuses
    /// `PathType` as the wrapper node in that case, but the only child is another
    /// `PathType`, not a `Path`.
    pub fn path(&self) -> Option<Path> {
        child_node(&self.0)
    }
    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        child_node(&self.0)
    }
}

/// `[T]` — a slice type. The single child type node is the element type.
pub struct SliceType(SyntaxNode);

impl AstNode for SliceType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SliceType
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

impl SliceType {
    /// The element type `T` in `[T]`.
    pub fn element_type(&self) -> Option<SyntaxNode> {
        child_type_node(&self.0)
    }
}

pub struct GenericArgList(SyntaxNode);

impl AstNode for GenericArgList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::GenericArgList
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

impl GenericArgList {
    pub fn args(&self) -> Vec<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| is_type_kind(n.kind()))
            .collect()
    }
}

pub struct ErrorUnionType(SyntaxNode);

impl AstNode for ErrorUnionType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ErrorUnionType
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

impl ErrorUnionType {
    pub fn error_type(&self) -> Option<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .find(|n| is_type_kind(n.kind()))
    }
    pub fn success_type(&self) -> Option<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| is_type_kind(n.kind()))
            .nth(1)
    }
}

pub struct ErrorSetUnionType(SyntaxNode);

impl AstNode for ErrorSetUnionType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ErrorSetUnionType
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

impl ErrorSetUnionType {
    pub fn members(&self) -> Vec<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| is_type_kind(n.kind()))
            .collect()
    }
}

/// Reserved for `dyn Trait` dynamic dispatch types. Not yet implemented in the
/// grammar; this node kind is never produced by the current parser.
pub struct DynType(SyntaxNode);

impl AstNode for DynType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::DynType
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

impl DynType {
    pub fn inner_type(&self) -> Option<SyntaxNode> {
        child_type_node(&self.0)
    }
}

pub struct UnitType(SyntaxNode);

impl AstNode for UnitType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::UnitType
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

/// Reserved for first-class function types (`fn(T) -> U`). Not yet implemented
/// in the grammar; this node kind is never produced by the current parser.
pub struct FnType(SyntaxNode);

impl AstNode for FnType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FnType
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

impl FnType {
    pub fn params(&self) -> Option<FnTypeParams> {
        child_node(&self.0)
    }
    pub fn ret_type(&self) -> Option<RetType> {
        child_node(&self.0)
    }
}

/// Parameter list for a first-class function type (`fn(T, U)`). Not yet
/// implemented in the grammar; this node kind is never produced today.
pub struct FnTypeParams(SyntaxNode);

impl AstNode for FnTypeParams {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FnTypeParams
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

impl FnTypeParams {
    pub fn types(&self) -> Vec<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| is_type_kind(n.kind()))
            .collect()
    }
}
