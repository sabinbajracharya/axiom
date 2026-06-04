//! Typed views for the source root and top-level item definitions.

use super::*;

// ── Root ──────────────────────────────────────────────────────────────────────

pub struct SourceFile(SyntaxNode);

impl AstNode for SourceFile {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SourceFile
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

impl SourceFile {
    /// All top-level item nodes (FnDef, StructDef, …).
    pub fn items(&self) -> Vec<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| n.kind() != SyntaxKind::Error)
            .collect()
    }
}

// ── Items ─────────────────────────────────────────────────────────────────────

pub struct FnDef(SyntaxNode);

impl AstNode for FnDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FnDef
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

impl FnDef {
    pub fn visibility(&self) -> Option<Visibility> {
        child_node(&self.0)
    }
    pub fn name(&self) -> Option<Name> {
        child_node(&self.0)
    }
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child_node(&self.0)
    }
    pub fn param_list(&self) -> Option<ParamList> {
        child_node(&self.0)
    }
    pub fn ret_type(&self) -> Option<RetType> {
        child_node(&self.0)
    }
    pub fn body(&self) -> Option<BlockExpr> {
        child_node(&self.0)
    }
}

pub struct StructDef(SyntaxNode);

impl AstNode for StructDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::StructDef
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

impl StructDef {
    pub fn visibility(&self) -> Option<Visibility> {
        child_node(&self.0)
    }
    pub fn name(&self) -> Option<Name> {
        child_node(&self.0)
    }
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child_node(&self.0)
    }
    pub fn field_list(&self) -> Option<FieldList> {
        child_node(&self.0)
    }
}

pub struct EnumDef(SyntaxNode);

impl AstNode for EnumDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::EnumDef
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

impl EnumDef {
    pub fn visibility(&self) -> Option<Visibility> {
        child_node(&self.0)
    }
    pub fn name(&self) -> Option<Name> {
        child_node(&self.0)
    }
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child_node(&self.0)
    }
    pub fn variant_list(&self) -> Option<VariantList> {
        child_node(&self.0)
    }
}

pub struct TraitDef(SyntaxNode);

impl AstNode for TraitDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TraitDef
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

impl TraitDef {
    pub fn visibility(&self) -> Option<Visibility> {
        child_node(&self.0)
    }
    pub fn name(&self) -> Option<Name> {
        child_node(&self.0)
    }
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child_node(&self.0)
    }
    pub fn item_list(&self) -> Option<TraitItemList> {
        child_node(&self.0)
    }
}

pub struct ImplBlock(SyntaxNode);

impl AstNode for ImplBlock {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ImplBlock
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

impl ImplBlock {
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        child_node(&self.0)
    }
    /// The first type child: either the trait (for `impl Trait for Type`) or
    /// the subject (for `impl Type`). Use `trait_ty()` / `subject_ty()` for
    /// the specific roles once the grammar distinguishes them further.
    pub fn first_type(&self) -> Option<SyntaxNode> {
        child_type_node(&self.0)
    }
    /// All type children in order. For `impl Trait for Type` this returns
    /// `[Trait, Type]`; for `impl Type` it returns `[Type]`.
    pub fn types(&self) -> Vec<PathType> {
        child_nodes_of(&self.0)
    }
    pub fn assoc_item_list(&self) -> Option<AssocItemList> {
        child_node(&self.0)
    }
}

pub struct ModDef(SyntaxNode);

impl AstNode for ModDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ModDef
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

impl ModDef {
    pub fn visibility(&self) -> Option<Visibility> {
        child_node(&self.0)
    }
    pub fn name(&self) -> Option<Name> {
        child_node(&self.0)
    }
    pub fn items(&self) -> Vec<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| n.kind() != SyntaxKind::Error)
            .collect()
    }
}

pub struct UseDecl(SyntaxNode);

impl AstNode for UseDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::UseDecl
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

impl UseDecl {
    pub fn visibility(&self) -> Option<Visibility> {
        child_node(&self.0)
    }
    pub fn use_tree(&self) -> Option<UseTree> {
        child_node(&self.0)
    }
}

pub struct ConstDef(SyntaxNode);

impl AstNode for ConstDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ConstDef
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

impl ConstDef {
    pub fn visibility(&self) -> Option<Visibility> {
        child_node(&self.0)
    }
    pub fn name(&self) -> Option<Name> {
        child_node(&self.0)
    }
    pub fn ty(&self) -> Option<SyntaxNode> {
        child_type_node(&self.0)
    }
    pub fn value(&self) -> Option<SyntaxNode> {
        child_expr_node(&self.0)
    }
}

pub struct ErrorSetDef(SyntaxNode);

impl AstNode for ErrorSetDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ErrorSetDef
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

impl ErrorSetDef {
    pub fn visibility(&self) -> Option<Visibility> {
        child_node(&self.0)
    }
    pub fn name(&self) -> Option<Name> {
        child_node(&self.0)
    }
    pub fn variant_list(&self) -> Option<ErrorVariantList> {
        child_node(&self.0)
    }
}

pub struct SubscriptDef(SyntaxNode);

impl AstNode for SubscriptDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SubscriptDef
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

impl SubscriptDef {
    pub fn param_list(&self) -> Option<ParamList> {
        child_node(&self.0)
    }
    pub fn ret_type(&self) -> Option<RetType> {
        child_node(&self.0)
    }
    pub fn body(&self) -> Option<BlockExpr> {
        child_node(&self.0)
    }
}
