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
//! let hir = lower(&root, "fn main() { val x = 1 + 2 }", None);
//! let thir = check(hir);
//! let dump = serialize(&thir, None);
//! assert!(dump.contains("Bin"));
//! ```

mod coverage;
mod error;
pub mod exhaustiveness;
pub mod mono;
mod serialize;
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

/// Compile a set of `(module_name, source)` modules together into one `Thir`.
///
/// This is the **single multi-module pipeline**: structural lowering (with linear
/// DefIds across modules) → cross-module export building → name resolution →
/// type-checking the combined HIR. Single-file, project, and stdlib-backed test
/// compilation all funnel through here — they differ only in *which* modules they
/// pass. See `docs/stdlib-loading-unification.md`.
pub fn check_modules(modules: &[(&str, &str)]) -> Thir {
    use axiom_parser::ast::AstNode;

    type Lowered = (
        String,
        Vec<axiom_hir::Item>,
        Vec<axiom_hir::Def>,
        Vec<axiom_hir::HirDiagnostic>,
    );

    let mut lowered: Vec<Lowered> = Vec::new();
    let mut next_id = 0usize;
    for (name, source) in modules {
        let result = axiom_parser::parse(source);
        let Some(root) = axiom_parser::ast::SourceFile::cast(result.tree) else {
            continue;
        };
        let (items, defs, diags, nid) = axiom_hir::lower_structural(&root, source, next_id);
        next_id = nid;
        lowered.push(((*name).to_string(), items, defs, diags));
    }

    let export_input: Vec<(String, Vec<axiom_hir::Def>)> = lowered
        .iter()
        .map(|(name, _, defs, _)| (name.clone(), defs.clone()))
        .collect();
    let exports = axiom_hir::build_global_exports(&export_input);

    let mut all_items: Vec<axiom_hir::Item> = Vec::new();
    let mut all_diags: Vec<axiom_hir::HirDiagnostic> = Vec::new();
    for (name, items, defs, diags) in &mut lowered {
        let mut items = std::mem::take(items);
        let mut diagnostics = std::mem::take(diags);
        axiom_hir::resolve_with_globals(&mut items, defs, &mut diagnostics, &exports, name);
        all_diags.append(&mut diagnostics);
        all_items.append(&mut items);
    }

    let hir = axiom_hir::Hir {
        items: all_items,
        diagnostics: all_diags,
    };
    check(hir)
}

/// Bare type-check — the deliberate, **labeled** no-stdlib mode: the user source
/// as one module with NO stdlib loaded. For compiler-isolation unit tests and the
/// floor built-ins that legitimately stay. It is the *same* `check_modules`
/// pipeline with an empty stdlib input (module name `""`), not a separate path —
/// so it cannot diverge. See `docs/stdlib-loading-unification.md` §3.
pub fn check_source(source: &str) -> Thir {
    check_modules(&[("", source)])
}
