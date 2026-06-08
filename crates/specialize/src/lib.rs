//! Monomorphization (specialization) pass: discovers every concrete
//! instantiation of generic functions used in the program and produces
//! one `MonoInstance` per unique `(fn_id, concrete_type_args)` pair.
//!
//! Consumed by `ir` lowering to generate specialized IR functions.
//!
//! The result types (`MonoResult`, `MonoInstance`) are defined in
//! `typecheck::mono_types` (to avoid circular deps) and re-exported here.

pub mod helpers;
mod mono;
mod walk;

pub use helpers::Substitution;
pub use mono::monomorphize;
pub use typecheck::mono_types::{MonoInstance, MonoResult};
