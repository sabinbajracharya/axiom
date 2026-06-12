//! Unit tests for the HIR desugar pass. Covers list literals, ?/else error
//! handling, idempotency, and variant coverage invariants. Split into submodules
//! to stay under the file-size cap; shared helpers live here.

use parser::ast::AstNode;
use resolver::hir_types::*;
use resolver::lang::LangItems;

fn test_lang_items() -> LangItems {
    LangItems {
        list: Some(HirId(100)),
        list_new: Some(HirId(101)),
        list_with_capacity: Some(HirId(102)),
        list_push: Some(HirId(103)),
    }
}

fn compile_and_desugar(source: &str) -> Hir {
    use parser::ast::AstNode;
    let result = parser::parse(source);
    let root = parser::ast::SourceFile::cast(result.tree).unwrap();
    let (items, _, _, next_id) = resolver::lower_structural(&root, source, 0);
    let lang_items = test_lang_items();
    let mut hir = Hir {
        items,
        diagnostics: Vec::new(),
    };
    super::pre_typecheck(&mut hir, &lang_items, next_id);
    hir
}

fn count_expr_kind(hir: &Hir, f: fn(&Expr) -> bool) -> usize {
    let mut count = 0;
    for item in &hir.items {
        count_item_expr_kind(item, f, &mut count);
    }
    count
}

fn count_item_expr_kind(item: &Item, f: fn(&Expr) -> bool, count: &mut usize) {
    match item {
        Item::FnDef(fd) => count_block_expr_kind(&fd.body, f, count),
        Item::ImplDef(i) => {
            for m in &i.methods {
                count_block_expr_kind(&m.body, f, count);
            }
        }
        _ => {}
    }
}

fn count_block_expr_kind(block: &Block, f: fn(&Expr) -> bool, count: &mut usize) {
    for stmt in &block.stmts {
        count_stmt_expr_kind(stmt, f, count);
    }
    if let Some(ref tail) = block.tail {
        count_one(tail, f, count);
        count_sub_expr_kind(tail, f, count);
    }
}

fn count_stmt_expr_kind(stmt: &Stmt, f: fn(&Expr) -> bool, count: &mut usize) {
    match stmt {
        Stmt::ValStmt(s) => {
            count_one(&s.value, f, count);
            count_sub_expr_kind(&s.value, f, count);
        }
        Stmt::VarStmt(s) => {
            count_one(&s.value, f, count);
            count_sub_expr_kind(&s.value, f, count);
        }
        Stmt::ExprStmt(s) => {
            count_one(&s.expr, f, count);
            count_sub_expr_kind(&s.expr, f, count);
        }
        Stmt::ReturnStmt(s) => {
            if let Some(ref v) = s.value {
                count_one(v, f, count);
                count_sub_expr_kind(v, f, count);
            }
        }
        _ => {}
    }
}

/// Count the number of sub-expressions matching `f` recursively.
///
/// This function is necessarily long because it matches every `Expr` variant
/// (enforced by `test_every_expr_variant_handled_by_desugar`). Each arm
/// recurses into child expressions; there is no shorter correct implementation.
#[allow(clippy::too_many_lines)]
fn count_sub_expr_kind(expr: &Expr, f: fn(&Expr) -> bool, count: &mut usize) {
    match expr {
        Expr::Lit(_) | Expr::Path(_) => {}
        Expr::Bin(e) => {
            count_one(e.left.as_ref(), f, count);
            count_sub_expr_kind(&e.left, f, count);
            count_one(e.right.as_ref(), f, count);
            count_sub_expr_kind(&e.right, f, count);
        }
        Expr::Call(e) => {
            for a in &e.args {
                count_one(a, f, count);
                count_sub_expr_kind(a, f, count);
            }
        }
        Expr::MethodCall(e) => {
            count_one(&e.receiver, f, count);
            count_sub_expr_kind(&e.receiver, f, count);
            for a in &e.args {
                count_one(a, f, count);
                count_sub_expr_kind(a, f, count);
            }
        }
        Expr::Block(e) => count_block_expr_kind(e, f, count),
        Expr::If(e) => {
            count_one(&e.condition, f, count);
            count_sub_expr_kind(&e.condition, f, count);
            count_block_expr_kind(&e.then_branch, f, count);
            if let Some(ref eb) = e.else_branch {
                count_one(eb, f, count);
                count_sub_expr_kind(eb, f, count);
            }
        }
        Expr::Match(e) => {
            count_one(&e.scrutinee, f, count);
            count_sub_expr_kind(&e.scrutinee, f, count);
            for arm in &e.arms {
                count_one(&arm.body, f, count);
                count_sub_expr_kind(&arm.body, f, count);
            }
        }
        Expr::Loop(e) => count_loop_sub_expr_kind(&e.kind, f, count),
        Expr::Unary(e) => {
            count_one(&e.operand, f, count);
            count_sub_expr_kind(&e.operand, f, count);
        }
        Expr::Field(e) => {
            count_one(&e.receiver, f, count);
            count_sub_expr_kind(&e.receiver, f, count);
        }
        Expr::Index(e) => {
            count_one(&e.base, f, count);
            count_sub_expr_kind(&e.base, f, count);
            for idx in &e.indices {
                count_one(idx, f, count);
                count_sub_expr_kind(idx, f, count);
            }
        }
        Expr::StructLit(e) => {
            for field in &e.fields {
                count_one(&field.value, f, count);
                count_sub_expr_kind(&field.value, f, count);
            }
        }
        Expr::Assign(e) => {
            count_sub_expr_kind(&e.value, f, count);
        }
        Expr::Question(e) => {
            count_sub_expr_kind(&e.expr, f, count);
        }
        Expr::Else(e) => {
            count_sub_expr_kind(&e.expr, f, count);
            count_sub_expr_kind(&e.fallback, f, count);
        }
        Expr::Catch(e) => {
            count_sub_expr_kind(&e.expr, f, count);
            count_sub_expr_kind(&e.fallback, f, count);
        }
        Expr::ListLit(e) => count_slice_exprs(&e.elements, f, count),
    }
}

fn count_slice_exprs(elements: &[Expr], f: fn(&Expr) -> bool, count: &mut usize) {
    for elem in elements {
        count_one(elem, f, count);
        count_sub_expr_kind(elem, f, count);
    }
}

fn count_one(expr: &Expr, f: fn(&Expr) -> bool, count: &mut usize) {
    if f(expr) {
        *count += 1;
    }
}

fn count_loop_sub_expr_kind(kind: &LoopKind, f: fn(&Expr) -> bool, count: &mut usize) {
    match kind {
        LoopKind::Infinite(b) => count_block_expr_kind(b, f, count),
        LoopKind::Conditional { condition, body } => {
            count_one(condition, f, count);
            count_sub_expr_kind(condition, f, count);
            count_block_expr_kind(body, f, count);
        }
        LoopKind::Iterator { iterable, body, .. } => {
            count_one(iterable, f, count);
            count_sub_expr_kind(iterable, f, count);
            count_block_expr_kind(body, f, count);
        }
    }
}

fn is_list_lit(expr: &Expr) -> bool {
    matches!(expr, Expr::ListLit(_))
}

fn is_call(expr: &Expr) -> bool {
    matches!(expr, Expr::Call(_))
}

fn is_method_call(expr: &Expr) -> bool {
    matches!(expr, Expr::MethodCall(_))
}

mod invariants;
mod list_and_errors;
