//! Monomorphization result types. The `monomorphize` algorithm lives in the
//! `specialize` crate; these data types live here to avoid a circular dependency
//! (specialize reads `Thir` → depends on typecheck).

use crate::types::Ty;
use resolver::HirId;

/// The output of monomorphization.
#[derive(Debug, Clone)]
pub struct MonoResult {
    pub instances: Vec<MonoInstance>,
}

/// A single monomorphized function instance.
#[derive(Debug, Clone)]
pub struct MonoInstance {
    pub name: String,
    pub original_name: String,
    pub type_args: Vec<Ty>,
    pub original_id: HirId,
    pub param_types: Vec<Ty>,
    pub return_type: Ty,
}
