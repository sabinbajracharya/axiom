//! The Axiom lower — structural lowering from CST/AST to HIR.
//!
//! Produces an HIR tree where every node has a stable `HirId` but names are
//! still `Unresolved`. Name resolution (`axiom-resolver`) fills them in.
//!
//! ```
//! use parser::parse;
//! use parser::ast::AstNode;
//! use lower::lower_structural;
//!
//! let result = parse("fn main() { val x = 1 }");
//! let root = parser::ast::SourceFile::cast(result.tree).unwrap();
//! let (items, defs, diags, _) = lower_structural(&root, "fn main() { val x = 1 }", 0);
//! assert!(!items.is_empty());
//! ```

pub mod error;
pub mod hir_types;
pub mod lowering;
pub mod serialize;

pub use error::HirDiagnostic;
pub use hir_types::*;
pub use lowering::{lower_structural, Def, DefKind};
pub use serialize::serialize;

/// Coverage checks: verifies that every `NameRef::Unresolved` in the HIR
/// has a corresponding `HirDiagnostic::UnresolvedName`. Returns `Ok(())`
/// if coverage is clean, or a list of coverage errors otherwise.
pub fn check_all(hir: &Hir) -> Result<(), Vec<CoverageError>> {
    let mut errors = Vec::new();
    let diagnosed: Vec<String> = hir
        .diagnostics
        .iter()
        .filter_map(|d| match d {
            HirDiagnostic::UnresolvedName { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    for item in &hir.items {
        check_item(item, &diagnosed, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_item(item: &Item, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    match item {
        Item::FnDef(f) => {
            check_block(&f.body, diagnosed, errors);
        }
        Item::StructDef(_)
        | Item::EnumDef(_)
        | Item::TraitDef(_)
        | Item::ImplDef(_)
        | Item::SubscriptDef(_)
        | Item::UseItem(_) => {}
    }
}

fn check_block(block: &Block, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    for stmt in &block.stmts {
        check_stmt(stmt, diagnosed, errors);
    }
    if let Some(tail) = &block.tail {
        check_expr(tail, diagnosed, errors);
    }
}

fn check_stmt(stmt: &Stmt, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    match stmt {
        Stmt::ValStmt(s) => check_expr(&s.value, diagnosed, errors),
        Stmt::VarStmt(s) => check_expr(&s.value, diagnosed, errors),
        Stmt::ExprStmt(s) => check_expr(&s.expr, diagnosed, errors),
        Stmt::ReturnStmt(s) => {
            if let Some(v) = &s.value {
                check_expr(v, diagnosed, errors);
            }
        }
        Stmt::BreakStmt(s) => {
            if let Some(v) = &s.value {
                check_expr(v, diagnosed, errors);
            }
        }
        Stmt::ContinueStmt(_) => {}
        Stmt::YieldStmt(s) => check_expr(&s.value, diagnosed, errors),
    }
}

fn check_expr(expr: &Expr, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    match expr {
        Expr::Path(p) => check_name_ref(&p.name_ref, p.id, diagnosed, errors),
        Expr::Call(c) => {
            check_name_ref(&c.callee, c.id, diagnosed, errors);
            for arg in &c.args {
                check_expr(arg, diagnosed, errors);
            }
        }
        Expr::Bin(b) => {
            check_expr(&b.left, diagnosed, errors);
            check_expr(&b.right, diagnosed, errors);
        }
        Expr::Unary(u) => check_expr(&u.operand, diagnosed, errors),
        Expr::MethodCall(m) => {
            check_expr(&m.receiver, diagnosed, errors);
            for arg in &m.args {
                check_expr(arg, diagnosed, errors);
            }
        }
        Expr::Field(f) => check_expr(&f.receiver, diagnosed, errors),
        Expr::Index(i) => {
            check_expr(&i.base, diagnosed, errors);
            for index in &i.indices {
                check_expr(index, diagnosed, errors);
            }
        }
        Expr::Block(b) => check_block(b, diagnosed, errors),
        Expr::If(i) => {
            check_expr(&i.condition, diagnosed, errors);
            check_block(&i.then_branch, diagnosed, errors);
            if let Some(els) = &i.else_branch {
                check_expr(els, diagnosed, errors);
            }
        }
        Expr::Match(m) => {
            check_expr(&m.scrutinee, diagnosed, errors);
            for arm in &m.arms {
                check_expr(&arm.body, diagnosed, errors);
            }
        }
        Expr::Loop(l) => check_loop(l, diagnosed, errors),
        Expr::StructLit(s) => {
            check_name_ref(&s.type_name, s.id, diagnosed, errors);
            for f in &s.fields {
                check_expr(&f.value, diagnosed, errors);
            }
        }
        Expr::Assign(a) => {
            check_assign_target(&a.target, a.id, diagnosed, errors);
            check_expr(&a.value, diagnosed, errors);
        }
        Expr::ListLit(l) => {
            for elem in &l.elements {
                check_expr(elem, diagnosed, errors);
            }
        }
        Expr::Lit(_) => {}
    }
}

fn check_loop(l: &LoopExpr, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    match &l.kind {
        LoopKind::Infinite(body) => check_block(body, diagnosed, errors),
        LoopKind::Conditional { condition, body } => {
            check_expr(condition, diagnosed, errors);
            check_block(body, diagnosed, errors);
        }
        LoopKind::Iterator { iterable, body, .. } => {
            check_expr(iterable, diagnosed, errors);
            check_block(body, diagnosed, errors);
        }
    }
}

fn check_assign_target(
    target: &AssignTarget,
    id: HirId,
    diagnosed: &[String],
    errors: &mut Vec<CoverageError>,
) {
    if let AssignTarget::Name(NameRef::Unresolved(u)) = target {
        if !diagnosed.contains(&u.text) {
            errors.push(CoverageError::UnresolvedWithoutDiagnostic {
                name: u.text.clone(),
                id,
            });
        }
    }
}

fn check_name_ref(nr: &NameRef, id: HirId, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    if let NameRef::Unresolved(u) = nr {
        if !diagnosed.contains(&u.text) {
            errors.push(CoverageError::UnresolvedWithoutDiagnostic {
                name: u.text.clone(),
                id,
            });
        }
    }
}

/// A coverage error discovered by `check_all`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverageError {
    /// A `NameRef::Unresolved` in the HIR with no corresponding diagnostic.
    UnresolvedWithoutDiagnostic { name: String, id: HirId },
}
