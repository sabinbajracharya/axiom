//! HIR type representations: source-level types, type parameters, and instances.

use super::{HirId, NameRef};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum HirTy {
    Named(NameRef),
    TypeParam(HirTypeParam),
    Instance(InstanceTy),
    Unit,
    Tuple(Vec<HirTy>),
    Fn(FnTy),
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnTy {
    pub params: Vec<HirTy>,
    pub return_type: Box<HirTy>,
}

/// A type parameter declaration: `T`, `T: Ord`, `T: Equatable + Hashable`.
#[derive(Debug, Clone, PartialEq)]
pub struct HirTypeParam {
    pub id: HirId,
    pub name: String,
    pub bounds: Vec<HirTraitBound>,
}

/// A trait bound on a type parameter: `Ord`, `Equatable`, `Hashable`.
#[derive(Debug, Clone, PartialEq)]
pub struct HirTraitBound {
    pub name: NameRef,
}

/// A generic type instance: `List<Int>`, `Map<String, Bool>`, `Option<Pair<Int, Bool>>`.
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceTy {
    pub name: NameRef,
    pub args: Vec<HirTy>,
}
