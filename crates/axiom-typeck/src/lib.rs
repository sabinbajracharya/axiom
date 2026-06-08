//! The Axiom type checker (M2): walks the HIR, assigns a type to every
//! expression and statement, and collects type diagnostics.
//!
//! Built test-first against [`docs/typeck-testing.md`](../../docs/typeck-testing.md).
//!
//! The type checker consumes a resolved HIR (from `axiom-hir`) and produces a
//! THIR (Typed HIR) — the same tree, annotated with a `TypeMap` side table and
//! unified diagnostics. Multi-module orchestration lives in `axiom-driver`; this
//! crate is a pure type-checking pass.
//!
//! ```
//! use axiom_parser::parse;
//! use axiom_parser::ast::AstNode;
//! use axiom_hir::lower;
//! use axiom_typeck::check;
//!
//! let source = "fn main() { val x = 1 + 2 }";
//! let result = parse(source);
//! let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
//! let hir = lower(&root, source, None);
//! let thir = check(hir);
//! assert!(thir.types.values().any(|t| matches!(t, axiom_typeck::Ty::Int)));
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
pub use error::{Diagnostic, TypeDiagnostic};
pub use mono::{monomorphize, MonoInstance, MonoResult};
pub use serialize::serialize;
pub use thir::{Thir, TypeMap};
pub use typeck::{check, check_with_lang_items};
pub use types::{EnumTy, FnTy, StructTy, Ty, TypeParamId};

/// Bare type-check — the deliberate, **labeled** no-stdlib mode: the user source
/// as one module with NO stdlib loaded. Parses + lowers + resolves + type-checks
/// in one call for compiler-isolation unit tests. See
/// `docs/stdlib-loading-unification.md` §3.
pub fn check_source(source: &str) -> Thir {
    use axiom_parser::ast::AstNode;
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree);
    let Some(root) = root else {
        return Thir {
            hir: axiom_hir::Hir {
                items: Vec::new(),
                diagnostics: Vec::new(),
            },
            types: TypeMap::new(),
            diagnostics: Vec::new(),
        };
    };
    let hir = axiom_hir::lower(&root, source, None);
    check(hir)
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
        axiom_hir::Stmt::ReturnStmt(s) => s.value.as_ref().map_or(0, expr_max_id),
        axiom_hir::Stmt::BreakStmt(s) => s.value.as_ref().map_or(0, expr_max_id),
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
            max = max
                .max(expr_max_id(&e.condition))
                .max(block_max_id(&e.then_branch));
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
        axiom_hir::Expr::Loop(e) => max = max.max(loop_max_id(&e.kind)),
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

fn loop_max_id(kind: &axiom_hir::LoopKind) -> usize {
    match kind {
        axiom_hir::LoopKind::Infinite(b) => block_max_id(b),
        axiom_hir::LoopKind::Conditional { condition, body } => {
            expr_max_id(condition).max(block_max_id(body))
        }
        axiom_hir::LoopKind::Iterator { iterable, body, .. } => {
            expr_max_id(iterable).max(block_max_id(body))
        }
    }
}
