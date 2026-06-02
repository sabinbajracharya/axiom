//! Typed AST views over the lossless CST (`docs/parser-testing.md` §2.4).
//!
//! Each struct is a thin wrapper over `SyntaxNode` — no data of its own.
//! `AstNode` is the common interface; each view adds accessor methods that
//! navigate to immediate children by kind, skipping trivia.
//!
//! The compiler consumes this layer and never sees trivia; the formatter
//! consumes the raw red tree and sees everything.

use crate::syntax::{SyntaxElement, SyntaxNode, SyntaxToken};
use crate::syntax_kind::SyntaxKind;

// ── Trait ────────────────────────────────────────────────────────────────────

/// The common interface for every typed AST view.
pub trait AstNode: Sized {
    /// Whether a `SyntaxNode` of this `kind` can be wrapped as `Self`.
    fn can_cast(kind: SyntaxKind) -> bool;
    /// Wrap `node` as `Self`, returning `None` if the kind does not match.
    fn cast(node: SyntaxNode) -> Option<Self>;
    /// The underlying red-tree node.
    fn syntax(&self) -> &SyntaxNode;
}

// ── Navigation helpers ────────────────────────────────────────────────────────

/// First child node that casts successfully to `N`.
fn child_node<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
    parent.child_nodes().into_iter().find_map(N::cast)
}

/// All child nodes that cast successfully to `N`.
fn child_nodes_of<N: AstNode>(parent: &SyntaxNode) -> Vec<N> {
    parent
        .child_nodes()
        .into_iter()
        .filter_map(N::cast)
        .collect()
}

/// First non-trivia token child with a specific `kind`.
fn child_token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    parent.children().into_iter().find_map(|e| match e {
        SyntaxElement::Token(t) if t.kind() == kind => Some(t),
        _ => None,
    })
}

/// First non-trivia token child, any kind.
fn first_token(parent: &SyntaxNode) -> Option<SyntaxToken> {
    parent.children().into_iter().find_map(|e| match e {
        SyntaxElement::Token(t) if !t.kind().is_trivia() => Some(t),
        _ => None,
    })
}

/// First child node whose kind is in the pattern family.
fn child_pat_node(parent: &SyntaxNode) -> Option<SyntaxNode> {
    parent.child_nodes().into_iter().find(|n| is_pat(n.kind()))
}

/// First child node whose kind is in the type family.
fn child_type_node(parent: &SyntaxNode) -> Option<SyntaxNode> {
    parent
        .child_nodes()
        .into_iter()
        .find(|n| is_type_kind(n.kind()))
}

/// First child node whose kind is in the expression family.
fn child_expr_node(parent: &SyntaxNode) -> Option<SyntaxNode> {
    parent
        .child_nodes()
        .into_iter()
        .find(|n| is_expr_kind(n.kind()))
}

/// All child nodes whose kind is in the expression family.
fn child_expr_nodes(parent: &SyntaxNode) -> Vec<SyntaxNode> {
    parent
        .child_nodes()
        .into_iter()
        .filter(|n| is_expr_kind(n.kind()))
        .collect()
}

fn is_pat(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::WildcardPat
            | SyntaxKind::LiteralPat
            | SyntaxKind::IdentPat
            | SyntaxKind::TupleStructPat
            | SyntaxKind::StructPat
            | SyntaxKind::PathPat
            | SyntaxKind::OrPat
            | SyntaxKind::RestPat
            | SyntaxKind::RangePat
            | SyntaxKind::Error
    )
}

fn is_type_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PathType
            | SyntaxKind::ErrorUnionType
            | SyntaxKind::ErrorSetUnionType
            | SyntaxKind::DynType
            | SyntaxKind::UnitType
            | SyntaxKind::FnType
            | SyntaxKind::Error
    )
}

fn is_expr_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BlockExpr
            | SyntaxKind::LiteralExpr
            | SyntaxKind::PathExpr
            | SyntaxKind::BinExpr
            | SyntaxKind::PrefixExpr
            | SyntaxKind::CallExpr
            | SyntaxKind::MethodCallExpr
            | SyntaxKind::FieldExpr
            | SyntaxKind::IndexExpr
            | SyntaxKind::ParenExpr
            | SyntaxKind::IfExpr
            | SyntaxKind::MatchExpr
            | SyntaxKind::LoopExpr
            | SyntaxKind::ClosureExpr
            | SyntaxKind::StructLitExpr
            | SyntaxKind::CastExpr
            | SyntaxKind::RangeExpr
            | SyntaxKind::TryExpr
            | SyntaxKind::AssignExpr
            | SyntaxKind::CatchExpr
            | SyntaxKind::ScopeExpr
            | SyntaxKind::SpawnExpr
            | SyntaxKind::ListLitExpr
            | SyntaxKind::ReturnStmt
            | SyntaxKind::BreakStmt
            | SyntaxKind::ContinueStmt
    )
}

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
            | SyntaxKind::DotDot
            | SyntaxKind::DotDotEq
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
        child_expr_nodes(&self.0).into_iter().nth(1)
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

// ── Loop forms ────────────────────────────────────────────────────────────────

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
    pub fn start(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }
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
    pub fn path(&self) -> Option<Path> {
        child_node(&self.0)
    }
    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        child_node(&self.0)
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // Tests legitimately use unwrap/expect/panic. RUST_CONVENTIONS §3.4.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::green::GreenNodeBuilder;
    use crate::parse;

    // ── Consistency test infrastructure ──────────────────────────────────────

    fn can_cast_item(kind: SyntaxKind) -> bool {
        FnDef::can_cast(kind)
            || StructDef::can_cast(kind)
            || EnumDef::can_cast(kind)
            || TraitDef::can_cast(kind)
            || ImplBlock::can_cast(kind)
            || ModDef::can_cast(kind)
            || UseDecl::can_cast(kind)
            || ConstDef::can_cast(kind)
            || ErrorSetDef::can_cast(kind)
    }

    fn can_cast_item_part(kind: SyntaxKind) -> bool {
        Visibility::can_cast(kind)
            || ParamList::can_cast(kind)
            || Param::can_cast(kind)
            || SelfParam::can_cast(kind)
            || FieldList::can_cast(kind)
            || Field::can_cast(kind)
            || VariantList::can_cast(kind)
            || Variant::can_cast(kind)
            || VariantPayload::can_cast(kind)
            || GenericParamList::can_cast(kind)
            || GenericParam::can_cast(kind)
            || TraitBounds::can_cast(kind)
            || RetType::can_cast(kind)
            || UseTree::can_cast(kind)
            || UseGroup::can_cast(kind)
            || UseRename::can_cast(kind)
            || ErrorVariantList::can_cast(kind)
            || ErrorVariant::can_cast(kind)
            || AssocItemList::can_cast(kind)
            || TraitItemList::can_cast(kind)
    }

    fn can_cast_stmt(kind: SyntaxKind) -> bool {
        LetStmt::can_cast(kind)
            || ExprStmt::can_cast(kind)
            || ReturnStmt::can_cast(kind)
            || BreakStmt::can_cast(kind)
            || ContinueStmt::can_cast(kind)
            || ErrdeferStmt::can_cast(kind)
    }

    fn can_cast_expr(kind: SyntaxKind) -> bool {
        BlockExpr::can_cast(kind)
            || LiteralExpr::can_cast(kind)
            || PathExpr::can_cast(kind)
            || BinExpr::can_cast(kind)
            || PrefixExpr::can_cast(kind)
            || CallExpr::can_cast(kind)
            || MethodCallExpr::can_cast(kind)
            || FieldExpr::can_cast(kind)
            || IndexExpr::can_cast(kind)
            || ParenExpr::can_cast(kind)
            || IfExpr::can_cast(kind)
            || MatchExpr::can_cast(kind)
            || LoopExpr::can_cast(kind)
            || ClosureExpr::can_cast(kind)
            || StructLitExpr::can_cast(kind)
            || CastExpr::can_cast(kind)
            || RangeExpr::can_cast(kind)
            || TryExpr::can_cast(kind)
            || AssignExpr::can_cast(kind)
            || CatchExpr::can_cast(kind)
            || ScopeExpr::can_cast(kind)
            || SpawnExpr::can_cast(kind)
            || ListLitExpr::can_cast(kind)
            || ArgList::can_cast(kind)
    }

    fn can_cast_path_name(kind: SyntaxKind) -> bool {
        Path::can_cast(kind)
            || PathSegment::can_cast(kind)
            || Name::can_cast(kind)
            || NameRef::can_cast(kind)
    }

    fn can_cast_loop_match_misc(kind: SyntaxKind) -> bool {
        LoopCondition::can_cast(kind)
            || LoopIter::can_cast(kind)
            || LoopLabel::can_cast(kind)
            || MatchArmList::can_cast(kind)
            || MatchArm::can_cast(kind)
            || MatchGuard::can_cast(kind)
            || ClosureParamList::can_cast(kind)
            || ClosureParam::can_cast(kind)
            || StructLitFieldList::can_cast(kind)
            || StructLitField::can_cast(kind)
    }

    fn can_cast_pattern(kind: SyntaxKind) -> bool {
        WildcardPat::can_cast(kind)
            || LiteralPat::can_cast(kind)
            || IdentPat::can_cast(kind)
            || TupleStructPat::can_cast(kind)
            || StructPat::can_cast(kind)
            || PathPat::can_cast(kind)
            || OrPat::can_cast(kind)
            || RestPat::can_cast(kind)
            || RangePat::can_cast(kind)
            || StructPatFieldList::can_cast(kind)
            || StructPatField::can_cast(kind)
            || TuplePatFieldList::can_cast(kind)
    }

    fn can_cast_type(kind: SyntaxKind) -> bool {
        PathType::can_cast(kind)
            || GenericArgList::can_cast(kind)
            || ErrorUnionType::can_cast(kind)
            || ErrorSetUnionType::can_cast(kind)
            || DynType::can_cast(kind)
            || UnitType::can_cast(kind)
            || FnType::can_cast(kind)
            || FnTypeParams::can_cast(kind)
    }

    fn can_cast_any(kind: SyntaxKind) -> bool {
        SourceFile::can_cast(kind)
            || can_cast_item(kind)
            || can_cast_item_part(kind)
            || can_cast_stmt(kind)
            || can_cast_expr(kind)
            || can_cast_path_name(kind)
            || can_cast_loop_match_misc(kind)
            || can_cast_pattern(kind)
            || can_cast_type(kind)
    }

    // ── Consistency ───────────────────────────────────────────────────────────

    /// Every non-Error node kind must have a corresponding AST view.
    /// Adding a node kind to `SyntaxKind` without a view causes this test to
    /// fail; adding a view without registering it in `can_cast_any` also fails.
    #[test]
    fn test_ast_every_node_kind_covered() {
        for &kind in SyntaxKind::ALL {
            if kind.is_node() && kind != SyntaxKind::Error {
                assert!(
                    can_cast_any(kind),
                    "{kind:?} is a node kind but has no AST view — \
                     add one and register it in can_cast_any"
                );
            }
        }
    }

    /// `cast(node).syntax().kind() == node.kind()` for every non-Error node kind.
    #[test]
    fn test_ast_cast_round_trip() {
        let representative = [
            SyntaxKind::SourceFile,
            SyntaxKind::FnDef,
            SyntaxKind::StructDef,
            SyntaxKind::EnumDef,
            SyntaxKind::TraitDef,
            SyntaxKind::ImplBlock,
            SyntaxKind::ModDef,
            SyntaxKind::UseDecl,
            SyntaxKind::ConstDef,
            SyntaxKind::ErrorSetDef,
            SyntaxKind::LetStmt,
            SyntaxKind::ExprStmt,
            SyntaxKind::BinExpr,
            SyntaxKind::IfExpr,
            SyntaxKind::MatchExpr,
            SyntaxKind::Name,
            SyntaxKind::NameRef,
            SyntaxKind::Path,
            SyntaxKind::IdentPat,
            SyntaxKind::PathType,
        ];
        for kind in representative {
            let mut b = GreenNodeBuilder::new();
            b.start_node(kind);
            b.finish_node();
            let node = SyntaxNode::new_root(b.finish());
            assert!(can_cast_any(kind), "no view for {kind:?}");
            assert_eq!(
                node.kind(),
                kind,
                "builder produced wrong kind for {kind:?}"
            );
        }
    }

    // ── Per-view unit tests ───────────────────────────────────────────────────

    #[test]
    fn test_fn_def_cast_round_trip() {
        let mut b = GreenNodeBuilder::new();
        b.start_node(SyntaxKind::FnDef);
        b.finish_node();
        let node = SyntaxNode::new_root(b.finish());
        let view = FnDef::cast(node).expect("FnDef::cast must succeed");
        assert_eq!(view.syntax().kind(), SyntaxKind::FnDef);
    }

    #[test]
    fn test_fn_def_cast_rejects_wrong_kind() {
        let mut b = GreenNodeBuilder::new();
        b.start_node(SyntaxKind::StructDef);
        b.finish_node();
        let node = SyntaxNode::new_root(b.finish());
        assert!(FnDef::cast(node).is_none());
    }

    #[test]
    fn test_fn_def_name_accessor() {
        let result = parse("fn greet() {}");
        let fn_def = result
            .tree
            .child_nodes()
            .into_iter()
            .find_map(FnDef::cast)
            .expect("should have a FnDef");
        let name = fn_def.name().expect("FnDef should have a name");
        assert_eq!(name.text(), Some("greet".to_string()));
    }

    #[test]
    fn test_fn_def_name_skips_trivia() {
        // Trivia (whitespace) between `fn` and the name must not appear in
        // the Name node's token — name.text() returns the identifier text only.
        let result = parse("fn   spaced() {}");
        let fn_def = result
            .tree
            .child_nodes()
            .into_iter()
            .find_map(FnDef::cast)
            .expect("should have a FnDef");
        let name = fn_def.name().expect("FnDef should have a name");
        assert_eq!(name.text(), Some("spaced".to_string()));
    }

    #[test]
    fn test_fn_def_param_list() {
        let result = parse("fn add(x: i32, y: i32) -> i32 {}");
        let fn_def = result
            .tree
            .child_nodes()
            .into_iter()
            .find_map(FnDef::cast)
            .expect("should have a FnDef");
        let params = fn_def.param_list().expect("should have param list");
        assert_eq!(params.params().len(), 2);
    }

    #[test]
    fn test_fn_def_ret_type() {
        let result = parse("fn inc(x: i32) -> i32 { x }");
        let fn_def = result
            .tree
            .child_nodes()
            .into_iter()
            .find_map(FnDef::cast)
            .expect("should have a FnDef");
        assert!(fn_def.ret_type().is_some(), "should have a return type");
    }

    #[test]
    fn test_struct_def_name_and_fields() {
        let result = parse("struct Point { x: f64, y: f64 }");
        let def = result
            .tree
            .child_nodes()
            .into_iter()
            .find_map(StructDef::cast)
            .expect("should have a StructDef");
        assert_eq!(def.name().and_then(|n| n.text()), Some("Point".to_string()));
        let fields = def.field_list().expect("should have field list").fields();
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn test_let_stmt_accessors() {
        let result = parse("fn f() { val x: i32 = 1 }");
        let fn_def = result
            .tree
            .child_nodes()
            .into_iter()
            .find_map(FnDef::cast)
            .expect("FnDef");
        let body = fn_def.body().expect("body");
        let stmt = body
            .stmts()
            .into_iter()
            .find_map(LetStmt::cast)
            .expect("LetStmt");
        let kw = stmt.binding_kw().expect("binding keyword");
        assert_eq!(kw.kind(), SyntaxKind::KwVal);
        assert!(stmt.pattern().is_some(), "should have a pattern");
        assert!(stmt.ty().is_some(), "should have a type annotation");
        assert!(stmt.value().is_some(), "should have an initializer");
    }

    #[test]
    fn test_bin_expr_operands_and_operator() {
        let result = parse("fn f() { 1 + 2 }");
        let fn_def = result
            .tree
            .child_nodes()
            .into_iter()
            .find_map(FnDef::cast)
            .expect("FnDef");
        let body = fn_def.body().expect("body");
        // The expression statement wraps the BinExpr.
        let expr_stmt = body
            .stmts()
            .into_iter()
            .find_map(ExprStmt::cast)
            .expect("ExprStmt");
        let bin = BinExpr::cast(expr_stmt.expr().expect("ExprStmt has expr")).expect("BinExpr");
        assert!(bin.lhs().is_some(), "lhs should exist");
        assert!(bin.rhs().is_some(), "rhs should exist");
        let op = bin.op_token().expect("operator token");
        assert_eq!(op.kind(), SyntaxKind::Plus);
    }

    #[test]
    fn test_name_text() {
        let result = parse("fn hello() {}");
        let fn_def = result
            .tree
            .child_nodes()
            .into_iter()
            .find_map(FnDef::cast)
            .expect("FnDef");
        let name = fn_def.name().expect("name");
        assert_eq!(
            name.ident_token().map(|t| t.kind()),
            Some(SyntaxKind::Ident)
        );
        assert_eq!(name.text(), Some("hello".to_string()));
    }

    #[test]
    fn test_source_file_items_excludes_error_nodes() {
        // Error nodes in the source must not show up in `SourceFile::items()`.
        let result = parse("@ fn f() {}");
        let sf = SourceFile::cast(result.tree).expect("SourceFile");
        let items = sf.items();
        assert!(
            items.iter().all(|n| n.kind() != SyntaxKind::Error),
            "items() must exclude Error nodes"
        );
        assert_eq!(items.len(), 1, "only the fn should be an item");
    }
}
