//! The Axiom type checker (M2): walks the HIR, assigns a type to every
//! expression and statement, and collects type diagnostics.
//!
//! Built test-first against [`docs/typeck-testing.md`](../../docs/typeck-testing.md).
//!
//! The type checker consumes an HIR (from `axiom-hir`) and produces a THIR
//! (Typed HIR) — the same tree, annotated with a `TypeMap` side table and
//! type-check diagnostics.
//!
//! ```
//! use axiom_parser::parse;
//! use axiom_parser::ast::AstNode;
//! use axiom_hir::lower;
//! use axiom_typeck::{check, serialize};
//!
//! let result = parse("fn main() { val x = 1 + 2 }");
//! let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
//! let hir = lower(&root, "fn main() { val x = 1 + 2 }");
//! let thir = check(hir);
//! let dump = serialize(&thir, None);
//! assert!(dump.contains("Bin"));
//! ```

mod coverage;
mod error;
pub mod exhaustiveness;
pub mod mono;
mod serialize;
mod stdlib;
mod thir;
mod typeck;
mod types;

pub use coverage::{check_all, TypeckCoverageError};
pub use error::TypeDiagnostic;
pub use mono::{monomorphize, MonoInstance, MonoResult};
pub use serialize::serialize;
pub use thir::{Thir, TypeMap};
pub use typeck::check;
pub use types::{EnumTy, FnTy, StructTy, Ty, TypeParamId};

/// Type-check a source string with the standard library prepended.
/// The stdlib defines library types (List, Map, etc.) that replace compiler built-ins.
/// Source concatenation happens before parsing, so HirIds are allocated linearly
/// across stdlib + user source (no collision, no remapping).
#[allow(clippy::expect_used)]
pub fn check_source_with_stdlib(source: &str) -> Thir {
    use axiom_parser::ast::AstNode;
    let combined = stdlib::with_stdlib(source);
    let result = axiom_parser::parse(&combined);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).expect("valid parse tree");
    let hir = axiom_hir::lower(&root, &combined);
    check(hir)
}
