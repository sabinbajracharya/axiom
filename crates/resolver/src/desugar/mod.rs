//! HIR desugar pass: rewrites sugar expressions into core HIR nodes.
//!
//! The actual implementation now lives in `crates/desugar/`. This module
//! exists only for backward compatibility. New code should depend on the
//! `desugar` crate directly.
