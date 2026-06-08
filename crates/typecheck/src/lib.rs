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
//! use parser::parse;
//! use parser::ast::AstNode;
//! use resolver::lower;
//! use typecheck::check;
//!
//! let source = "fn main() { val x = 1 + 2 }";
//! let result = parse(source);
//! let root = parser::ast::SourceFile::cast(result.tree).unwrap();
//! let hir = lower(&root, source, None);
//! let thir = check(hir);
//! assert!(thir.types.values().any(|t| matches!(t, typecheck::Ty::Int)));
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
    use parser::ast::AstNode;
    let result = parser::parse(source);
    let root = parser::ast::SourceFile::cast(result.tree);
    let Some(root) = root else {
        return Thir {
            hir: resolver::Hir {
                items: Vec::new(),
                diagnostics: Vec::new(),
            },
            types: TypeMap::new(),
            diagnostics: Vec::new(),
        };
    };
    let hir = resolver::lower(&root, source, None);
    check(hir)
}

/// Find the highest `HirId` in an HIR, so the desugar pass can seed its fresh-ID
/// counter. Returns 0 for an empty HIR.
pub fn hir_max_id(hir: &resolver::Hir) -> usize {
    let mut max = 0;
    for item in &hir.items {
        max = max.max(item_max_id(item));
    }
    max
}

fn item_max_id(item: &resolver::Item) -> usize {
    let id = match item {
        resolver::Item::FnDef(f) => f.id.0,
        resolver::Item::StructDef(s) => s.id.0,
        resolver::Item::EnumDef(e) => e.id.0,
        resolver::Item::TraitDef(t) => t.id.0,
        resolver::Item::ImplDef(i) => i.id.0,
        resolver::Item::SubscriptDef(s) => s.id.0,
        resolver::Item::UseItem(u) => u.id.0,
    };
    let body_max = match item {
        resolver::Item::FnDef(f) => block_max_id(&f.body),
        resolver::Item::ImplDef(i) => {
            let mut m = 0;
            for method in &i.methods {
                m = m.max(block_max_id(&method.body));
            }
            for s in &i.subscripts {
                m = m.max(block_max_id(&s.body));
            }
            m
        }
        resolver::Item::TraitDef(t) => {
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

fn block_max_id(block: &resolver::Block) -> usize {
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

fn stmt_max_sub_id(stmt: &resolver::Stmt) -> usize {
    match stmt {
        resolver::Stmt::ValStmt(s) => expr_max_id(&s.value),
        resolver::Stmt::VarStmt(s) => expr_max_id(&s.value),
        resolver::Stmt::ExprStmt(s) => expr_max_id(&s.expr),
        resolver::Stmt::ReturnStmt(s) => s.value.as_ref().map_or(0, expr_max_id),
        resolver::Stmt::BreakStmt(s) => s.value.as_ref().map_or(0, expr_max_id),
        _ => 0,
    }
}

fn expr_max_id(expr: &resolver::Expr) -> usize {
    let mut max = expr.id().0;
    match expr {
        resolver::Expr::Lit(_) | resolver::Expr::Path(_) => {}
        resolver::Expr::Bin(e) => {
            max = max.max(expr_max_id(&e.left)).max(expr_max_id(&e.right));
        }
        resolver::Expr::Unary(e) => max = max.max(expr_max_id(&e.operand)),
        resolver::Expr::Call(e) => {
            for a in &e.args {
                max = max.max(expr_max_id(a));
            }
        }
        resolver::Expr::MethodCall(e) => {
            max = max.max(expr_max_id(&e.receiver));
            for a in &e.args {
                max = max.max(expr_max_id(a));
            }
        }
        resolver::Expr::Field(e) => max = max.max(expr_max_id(&e.receiver)),
        resolver::Expr::Index(e) => {
            max = max.max(expr_max_id(&e.base));
            for idx in &e.indices {
                max = max.max(expr_max_id(idx));
            }
        }
        resolver::Expr::Block(e) => max = max.max(block_max_id(e)),
        resolver::Expr::If(e) => {
            max = max
                .max(expr_max_id(&e.condition))
                .max(block_max_id(&e.then_branch));
            if let Some(ref eb) = e.else_branch {
                max = max.max(expr_max_id(eb));
            }
        }
        resolver::Expr::Match(e) => {
            max = max.max(expr_max_id(&e.scrutinee));
            for arm in &e.arms {
                if let Some(ref guard) = arm.guard {
                    max = max.max(expr_max_id(guard));
                }
                max = max.max(expr_max_id(&arm.body));
            }
        }
        resolver::Expr::Loop(e) => max = max.max(loop_max_id(&e.kind)),
        resolver::Expr::StructLit(e) => {
            for f in &e.fields {
                max = max.max(expr_max_id(&f.value));
            }
        }
        resolver::Expr::ListLit(e) => {
            for el in &e.elements {
                max = max.max(expr_max_id(el));
            }
        }
        resolver::Expr::Assign(e) => {
            max = max.max(expr_max_id(&e.value));
        }
    }
    max
}

fn loop_max_id(kind: &resolver::LoopKind) -> usize {
    match kind {
        resolver::LoopKind::Infinite(b) => block_max_id(b),
        resolver::LoopKind::Conditional { condition, body } => {
            expr_max_id(condition).max(block_max_id(body))
        }
        resolver::LoopKind::Iterator { iterable, body, .. } => {
            expr_max_id(iterable).max(block_max_id(body))
        }
    }
}
