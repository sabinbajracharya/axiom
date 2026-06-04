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
//! let dump = serialize(&thir);
//! assert!(dump.contains("Bin"));
//! ```

mod coverage;
mod error;
mod serialize;
mod thir;
mod typeck;
mod types;

pub use coverage::{check_all, TypeckCoverageError};
pub use error::TypeDiagnostic;
pub use serialize::serialize;
pub use thir::{Thir, TypeMap};
pub use typeck::check;
pub use types::{EnumTy, FnTy, StructTy, Ty};
