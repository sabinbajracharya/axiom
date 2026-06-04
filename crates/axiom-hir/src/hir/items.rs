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
}

#[derive(Debug, Clone)]
pub struct FnDef {
    pub id: HirId,
    pub name: String,
    pub visibility: Visibility,
    pub type_params: Vec<HirTypeParam>,
    pub params: Vec<Param>,
    pub return_type: Option<HirTy>,
    pub body: Block,
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
}
