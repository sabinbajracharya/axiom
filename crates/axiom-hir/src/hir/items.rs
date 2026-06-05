//! HIR item definitions: functions, structs, enums, traits, and impls.

use super::{Block, CallingConvention, HirId, HirTy, HirTypeParam, NameRef, Visibility};

// ── Items ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Item {
    FnDef(FnDef),
    StructDef(StructDef),
    EnumDef(EnumDef),
    TraitDef(TraitDef),
    ImplDef(ImplDef),
    SubscriptDef(SubscriptDef),
    UseItem(UseItem),
}

#[derive(Debug, Clone)]
pub struct FnDef {
    pub id: HirId,
    pub name: String,
    /// Module path for cross-module name qualification (e.g., "core::platform").
    /// Empty for single-file compilation. Set during multi-file name resolution.
    pub module_path: String,
    pub visibility: Visibility,
    pub type_params: Vec<HirTypeParam>,
    pub params: Vec<Param>,
    pub return_type: Option<HirTy>,
    pub body: Block,
    /// `Some("C")` for `extern "C" fn`, `Some("")` for `extern fn`, `None` for
    /// regular functions. Extern functions have no user-written body.
    pub extern_abi: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub id: HirId,
    pub convention: CallingConvention,
    pub name: String,
    pub ty: Option<HirTy>,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub id: HirId,
    pub name: String,
    pub visibility: Visibility,
    pub type_params: Vec<HirTypeParam>,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub id: HirId,
    pub name: String,
    pub ty: HirTy,
    pub visibility: Visibility,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub id: HirId,
    pub name: String,
    pub visibility: Visibility,
    pub type_params: Vec<HirTypeParam>,
    pub variants: Vec<VariantDef>,
}

#[derive(Debug, Clone)]
pub struct VariantDef {
    pub id: HirId,
    pub name: String,
    pub payload: Vec<HirTy>,
}

#[derive(Debug, Clone)]
pub struct TraitDef {
    pub id: HirId,
    pub name: String,
    pub visibility: Visibility,
    pub type_params: Vec<HirTypeParam>,
    pub methods: Vec<TraitMethod>,
}

/// A method declared in a trait. If `body` is `Some`, it's a default implementation.
#[derive(Debug, Clone)]
pub struct TraitMethod {
    pub id: HirId,
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<HirTy>,
    pub body: Option<Block>,
}

/// An impl block: `impl Shape for Circle { ... }` or `impl Circle { ... }`.
#[derive(Debug, Clone)]
pub struct ImplDef {
    pub id: HirId,
    pub trait_name: Option<NameRef>,
    pub type_name: NameRef,
    pub type_params: Vec<HirTypeParam>,
    pub methods: Vec<FnDef>,
    pub subscripts: Vec<SubscriptDef>,
}

/// A subscript declaration: `subscript(params) -> RetType { body }`.
/// No name — identified by parameter signature. Lives inside an impl block.
#[derive(Debug, Clone)]
pub struct SubscriptDef {
    pub id: HirId,
    pub params: Vec<Param>,
    pub return_type: Option<HirTy>,
    pub body: Block,
}

// ── Use items ─────────────────────────────────────────────────────────────────

/// A `use` declaration: `use foo::{bar, baz};`
#[derive(Debug, Clone)]
pub struct UseItem {
    pub id: HirId,
    pub visibility: Visibility,
    pub tree: UseTree,
}

/// A use tree: a path with either a single import, a group, or a glob.
#[derive(Debug, Clone)]
pub struct UseTree {
    /// Path segments (e.g. `["foo", "bar"]` for `foo::bar`).
    pub path: Vec<String>,
    /// What this tree imports.
    pub kind: UseTreeKind,
}

/// The kind of import a use tree represents.
#[derive(Debug, Clone)]
pub enum UseTreeKind {
    /// A single name import: `use foo::bar` or `use foo::bar as baz`.
    Single { rename: Option<String> },
    /// A grouped import: `use foo::{bar, baz}`.
    Group(Vec<UseTree>),
    /// A glob import: `use foo::*`.
    Glob,
}
