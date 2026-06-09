//! HIR desugaring: rewrites sugar expressions into core HIR nodes.
//!
//! Split into two passes with different dependencies:
//! - **Pre-typecheck** (`catch`, `else`, `ListLit`) — needs `LangItems`.
//! - **Post-typecheck** (`?`) — needs `TypeMap` to determine Option vs Result.
//!
//! The driver explicitly calls both passes. The typecheck crate does no
//! desugaring internally.

pub mod helpers;
pub mod post_typecheck;
pub mod pre_typecheck;

pub use post_typecheck::post_typecheck;
pub use pre_typecheck::pre_typecheck;
