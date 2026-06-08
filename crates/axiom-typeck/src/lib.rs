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
pub use typeck::{check, check_with_lang_items};
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
    // Lang-item bindings discovered in the stdlib; any `@lang` tag in a
    // non-stdlib module is rejected so user code can't hijack a lang item.
    let mut stdlib_bindings: Vec<axiom_hir::LangBinding> = Vec::new();
    let mut stdlib_present = false;
    for (name, items, defs, diags) in &mut lowered {
        let mut items = std::mem::take(items);
        let mut diagnostics = std::mem::take(diags);
        axiom_hir::resolve_with_globals(&mut items, defs, &mut diagnostics, &exports, name);
        let bindings = axiom_hir::collect_lang_bindings(&items);
        if is_stdlib_module(name) {
            stdlib_present = true;
            stdlib_bindings.extend(bindings);
        } else {
            for b in bindings {
                diagnostics.push(axiom_hir::HirDiagnostic::LangItemOutsideStdlib {
                    key: b.key,
                    span: axiom_lexer::Span { lo: 0, hi: 0 },
                });
            }
        }
        all_diags.append(&mut diagnostics);
        all_items.append(&mut items);
    }

    let (lang_items, mut lang_diags) =
        axiom_hir::resolve_lang_items(&stdlib_bindings, stdlib_present);
    all_diags.append(&mut lang_diags);

    let mut hir = axiom_hir::Hir {
        items: all_items,
        diagnostics: all_diags,
    };
    check_with_lang_items(hir, lang_items)
}

/// Whether a module path belongs to the embedded standard library. Lang-item
/// `@lang` tags are honored only here; everywhere else they are an error.
fn is_stdlib_module(name: &str) -> bool {
    name.starts_with("core") || name.starts_with("std")
}

/// Bare type-check — the deliberate, **labeled** no-stdlib mode: the user source
/// as one module with NO stdlib loaded. For compiler-isolation unit tests and the
/// floor built-ins that legitimately stay. It is the *same* `check_modules`
/// pipeline with an empty stdlib input (module name `""`), not a separate path —
/// so it cannot diverge. See `docs/stdlib-loading-unification.md` §3.
pub fn check_source(source: &str) -> Thir {
    check_modules(&[("", source)])
}

/// Find the highest `HirId` in an HIR, so the desugar pass can seed its fresh-ID
/// counter. Returns 0 for an empty HIR.
pub(crate) fn hir_max_id(hir: &axiom_hir::Hir) -> usize {
    let mut max = 0;
    for item in &hir.items {
        max = max.max(item_max_id(item));
    }
    max
}

fn item_max_id(item: &axiom_hir::Item) -> usize {
    let id = match item {
        axiom_hir::Item::FnDef(f) => f.id.0,
        axiom_hir::Item::StructDef(s) => s.id.0,
        axiom_hir::Item::EnumDef(e) => e.id.0,
        axiom_hir::Item::TraitDef(t) => t.id.0,
        axiom_hir::Item::ImplDef(i) => i.id.0,
        axiom_hir::Item::SubscriptDef(s) => s.id.0,
        axiom_hir::Item::UseItem(u) => u.id.0,
    };
    let body_max = match item {
        axiom_hir::Item::FnDef(f) => block_max_id(&f.body),
        axiom_hir::Item::ImplDef(i) => {
            let mut m = 0;
            for method in &i.methods {
                m = m.max(block_max_id(&method.body));
            }
            for s in &i.subscripts {
                m = m.max(block_max_id(&s.body));
            }
            m
        }
        axiom_hir::Item::TraitDef(t) => {
            let mut m = 0;
            for method in &t.methods {
                if let Some(ref body) = method.body {
                    m = m.max(block_max_id(body));
                }
            }
            m
        }
        _ => 0,
    };
    id.max(body_max)
}

fn block_max_id(block: &axiom_hir::Block) -> usize {
    let mut max = block.id.0;
    for stmt in &block.stmts {
        max = max.max(stmt.id().0);
        max = max.max(stmt_max_sub_id(stmt));
    }
    if let Some(ref tail) = block.tail {
        max = max.max(expr_max_id(tail));
    }
    max
}

fn stmt_max_sub_id(stmt: &axiom_hir::Stmt) -> usize {
    match stmt {
        axiom_hir::Stmt::ValStmt(s) => expr_max_id(&s.value),
        axiom_hir::Stmt::VarStmt(s) => expr_max_id(&s.value),
        axiom_hir::Stmt::ExprStmt(s) => expr_max_id(&s.expr),
        axiom_hir::Stmt::ReturnStmt(s) => s.value.as_ref().map_or(0, |v| expr_max_id(v)),
        axiom_hir::Stmt::BreakStmt(s) => s.value.as_ref().map_or(0, |v| expr_max_id(v)),
        _ => 0,
    }
}

fn expr_max_id(expr: &axiom_hir::Expr) -> usize {
    let mut max = expr.id().0;
    match expr {
        axiom_hir::Expr::Lit(_) | axiom_hir::Expr::Path(_) => {}
        axiom_hir::Expr::Bin(e) => {
            max = max.max(expr_max_id(&e.left)).max(expr_max_id(&e.right));
        }
        axiom_hir::Expr::Unary(e) => max = max.max(expr_max_id(&e.operand)),
        axiom_hir::Expr::Call(e) => {
            for a in &e.args {
                max = max.max(expr_max_id(a));
            }
        }
        axiom_hir::Expr::MethodCall(e) => {
            max = max.max(expr_max_id(&e.receiver));
            for a in &e.args {
                max = max.max(expr_max_id(a));
            }
        }
        axiom_hir::Expr::Field(e) => max = max.max(expr_max_id(&e.receiver)),
        axiom_hir::Expr::Index(e) => {
            max = max.max(expr_max_id(&e.base));
            for idx in &e.indices {
                max = max.max(expr_max_id(idx));
            }
        }
        axiom_hir::Expr::Block(e) => max = max.max(block_max_id(e)),
        axiom_hir::Expr::If(e) => {
            max = max.max(expr_max_id(&e.condition)).max(block_max_id(&e.then_branch));
            if let Some(ref eb) = e.else_branch {
                max = max.max(expr_max_id(eb));
            }
        }
        axiom_hir::Expr::Match(e) => {
            max = max.max(expr_max_id(&e.scrutinee));
            for arm in &e.arms {
                if let Some(ref guard) = arm.guard {
                    max = max.max(expr_max_id(guard));
                }
                max = max.max(expr_max_id(&arm.body));
            }
        }
        axiom_hir::Expr::Loop(e) => match &e.kind {
            axiom_hir::LoopKind::Infinite(b) => max = max.max(block_max_id(b)),
            axiom_hir::LoopKind::Conditional { condition, body } => {
                max = max.max(expr_max_id(condition)).max(block_max_id(body));
            }
            axiom_hir::LoopKind::Iterator {
                iterable, body, ..
            } => {
                max = max.max(expr_max_id(iterable)).max(block_max_id(body));
            }
        },
        axiom_hir::Expr::StructLit(e) => {
            for f in &e.fields {
                max = max.max(expr_max_id(&f.value));
            }
        }
        axiom_hir::Expr::ListLit(e) => {
            for el in &e.elements {
                max = max.max(expr_max_id(el));
            }
        }
        axiom_hir::Expr::Assign(e) => {
            max = max.max(expr_max_id(&e.value));
        }
    }
    max
}
