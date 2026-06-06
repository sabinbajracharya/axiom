//! Typed views for item parts (params, fields, variants, generics, use-trees).

use super::*;

// ── Item parts ────────────────────────────────────────────────────────────────

pub struct Visibility(SyntaxNode);

impl AstNode for Visibility {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Visibility
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

impl Visibility {
    pub fn pub_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::KwPub)
    }
}

/// A list of leading attributes on an item (`@lang("list") @other(...)`).
pub struct AttrList(SyntaxNode);

impl AstNode for AttrList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::AttrList
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

impl AttrList {
    pub fn attrs(&self) -> Vec<Attr> {
        child_nodes_of(&self.0)
    }
}

/// One attribute: `@ name ( "arg" )`, e.g. `@lang("list")`.
pub struct Attr(SyntaxNode);

impl AstNode for Attr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Attr
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

impl Attr {
    /// The attribute name (e.g. `lang`).
    pub fn name(&self) -> Option<String> {
        child_node::<Name>(&self.0).and_then(|n| n.text())
    }

    /// The decoded string argument (e.g. `list` for `@lang("list")`), without
    /// the surrounding quotes. `None` when the attribute has no string argument.
    pub fn arg(&self) -> Option<String> {
        let token = child_token(&self.0, SyntaxKind::StrLit)?;
        let text = token.text();
        if text.len() < 2 {
            return None;
        }
        Some(text[1..text.len() - 1].to_string())
    }
}

pub struct ParamList(SyntaxNode);

impl AstNode for ParamList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ParamList
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

impl ParamList {
    pub fn self_param(&self) -> Option<SelfParam> {
        child_node(&self.0)
    }
    pub fn params(&self) -> Vec<Param> {
        child_nodes_of(&self.0)
    }
}

pub struct Param(SyntaxNode);

impl AstNode for Param {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Param
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

impl Param {
    /// The calling-convention keyword (`let`, `inout`, or `sink`), if present.
    pub fn convention_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::KwLet)
            .or_else(|| child_token(&self.0, SyntaxKind::KwInout))
            .or_else(|| child_token(&self.0, SyntaxKind::KwSink))
    }
    pub fn name_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Ident)
    }
    pub fn ty(&self) -> Option<SyntaxNode> {
        child_type_node(&self.0)
    }
}

pub struct SelfParam(SyntaxNode);

impl AstNode for SelfParam {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SelfParam
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

impl SelfParam {
    /// The calling-convention keyword (`let`, `inout`, or `sink`), if present.
    pub fn convention_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::KwLet)
            .or_else(|| child_token(&self.0, SyntaxKind::KwInout))
            .or_else(|| child_token(&self.0, SyntaxKind::KwSink))
    }
    pub fn self_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::KwSelf)
    }
}

pub struct FieldList(SyntaxNode);

impl AstNode for FieldList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FieldList
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

impl FieldList {
    pub fn fields(&self) -> Vec<Field> {
        child_nodes_of(&self.0)
    }
}

pub struct Field(SyntaxNode);

impl AstNode for Field {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Field
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

impl Field {
    pub fn visibility(&self) -> Option<Visibility> {
        child_node(&self.0)
    }
    pub fn name_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Ident)
    }
    pub fn ty(&self) -> Option<SyntaxNode> {
        child_type_node(&self.0)
    }
}

pub struct VariantList(SyntaxNode);

impl AstNode for VariantList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::VariantList
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

impl VariantList {
    pub fn variants(&self) -> Vec<Variant> {
        child_nodes_of(&self.0)
    }
}

pub struct Variant(SyntaxNode);

impl AstNode for Variant {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Variant
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

impl Variant {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Ident)
    }
    pub fn payload(&self) -> Option<VariantPayload> {
        child_node(&self.0)
    }
}

pub struct VariantPayload(SyntaxNode);

impl AstNode for VariantPayload {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::VariantPayload
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

impl VariantPayload {
    pub fn types(&self) -> Vec<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| is_type_kind(n.kind()))
            .collect()
    }
}

pub struct GenericParamList(SyntaxNode);

impl AstNode for GenericParamList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::GenericParamList
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

impl GenericParamList {
    pub fn params(&self) -> Vec<GenericParam> {
        child_nodes_of(&self.0)
    }
}

pub struct GenericParam(SyntaxNode);

impl AstNode for GenericParam {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::GenericParam
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

impl GenericParam {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Ident)
    }
    pub fn bounds(&self) -> Option<TraitBounds> {
        child_node(&self.0)
    }
}

pub struct TraitBounds(SyntaxNode);

impl AstNode for TraitBounds {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TraitBounds
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

impl TraitBounds {
    pub fn types(&self) -> Vec<SyntaxNode> {
        self.0
            .child_nodes()
            .into_iter()
            .filter(|n| is_type_kind(n.kind()))
            .collect()
    }
}

pub struct RetType(SyntaxNode);

impl AstNode for RetType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RetType
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

impl RetType {
    pub fn ty(&self) -> Option<SyntaxNode> {
        child_type_node(&self.0)
    }
}

pub struct UseTree(SyntaxNode);

impl AstNode for UseTree {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::UseTree
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

impl UseTree {
    pub fn group(&self) -> Option<UseGroup> {
        child_node(&self.0)
    }
    pub fn rename(&self) -> Option<UseRename> {
        child_node(&self.0)
    }
}

pub struct UseGroup(SyntaxNode);

impl AstNode for UseGroup {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::UseGroup
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

impl UseGroup {
    pub fn trees(&self) -> Vec<UseTree> {
        child_nodes_of(&self.0)
    }
}

pub struct UseRename(SyntaxNode);

impl AstNode for UseRename {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::UseRename
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

impl UseRename {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Ident)
    }
}

pub struct ErrorVariantList(SyntaxNode);

impl AstNode for ErrorVariantList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ErrorVariantList
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

impl ErrorVariantList {
    pub fn variants(&self) -> Vec<ErrorVariant> {
        child_nodes_of(&self.0)
    }
}

pub struct ErrorVariant(SyntaxNode);

impl AstNode for ErrorVariant {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::ErrorVariant
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

impl ErrorVariant {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        child_token(&self.0, SyntaxKind::Ident)
    }
}

pub struct AssocItemList(SyntaxNode);

impl AstNode for AssocItemList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::AssocItemList
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

impl AssocItemList {
    pub fn methods(&self) -> Vec<FnDef> {
        child_nodes_of(&self.0)
    }
    pub fn subscripts(&self) -> Vec<SubscriptDef> {
        child_nodes_of(&self.0)
    }
}

pub struct TraitItemList(SyntaxNode);

impl AstNode for TraitItemList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TraitItemList
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

impl TraitItemList {
    pub fn methods(&self) -> Vec<FnDef> {
        child_nodes_of(&self.0)
    }
}
