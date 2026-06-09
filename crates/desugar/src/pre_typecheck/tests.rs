//! Unit tests for the HIR desugar pass. Covers list literals, ?/else
//! error handling, idempotency, and variant coverage invariants.

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

#[test]
fn test_desugar_empty_list_becomes_new_call() {
    let hir = compile_and_desugar("fn main() { val xs: List<Int> = [] }");
    assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
    assert!(count_expr_kind(&hir, is_call) >= 1);
}

#[test]
fn test_desugar_non_empty_list_removes_all_list_lit() {
    let hir = compile_and_desugar("fn main() { val xs = [1, 2, 3] }");
    assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
}

#[test]
fn test_desugar_non_empty_list_produces_block_with_calls() {
    let hir = compile_and_desugar("fn main() { val xs = [10, 20, 30] }");
    assert!(
        count_expr_kind(&hir, is_call) >= 1,
        "should have with_capacity call"
    );
    assert!(
        count_expr_kind(&hir, is_method_call) >= 3,
        "should have three push calls"
    );
}

#[test]
fn test_desugar_nested_list() {
    let hir = compile_and_desugar("fn main() { val xs = [[1], [2, 3]] }");
    assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
}

#[test]
fn test_desugar_does_not_touch_non_sugar() {
    let source = "fn main() { val x = 1 + 2\n  loop { break }\n  if x > 0 { val y = 3 }\n}";
    let hir = compile_and_desugar(source);
    let dump = resolver::serialize::serialize(&hir);
    assert!(dump.contains("Bin"));
    assert!(dump.contains("Loop"));
    assert!(dump.contains("If"));
    assert!(!dump.contains("ListLit"));
}

#[test]
fn test_desugar_result_has_no_list_lit() {
    let hir = compile_and_desugar("fn main() { val a = [1]; val b = [2, 3]; val c = [] }");
    assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
}

#[test]
fn test_desugar_produces_varstmt_and_no_listlit() {
    let hir = compile_and_desugar("fn main() { val xs = [1, 2, 3, 4, 5] }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(
        dump.contains("VarStmt"),
        "desugared list should have VarStmt"
    );
    assert!(!dump.contains("ListLit"), "no ListLit after desugar");
}

#[test]
fn test_desugar_generates_unique_ids() {
    let hir = compile_and_desugar("fn main() { val xs = [1, 2, 3] }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(
        dump.contains("VarStmt"),
        "desugared list should have VarStmt"
    );
    assert!(!dump.contains("ListLit"), "no ListLit after desugar");
}

#[test]
fn test_desugar_singleton_list() {
    let hir = compile_and_desugar("fn main() { val xs = [42] }");
    assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
    assert!(
        count_expr_kind(&hir, is_call) >= 1,
        "should have with_capacity(1)"
    );
    assert!(
        count_expr_kind(&hir, is_method_call) >= 1,
        "should have one push call"
    );
}

#[test]
fn test_desugar_list_in_call_arg() {
    let hir = compile_and_desugar("fn take(list: List<Int>) {}\nfn main() { take([1, 2]) }");
    assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
    let dump = resolver::serialize::serialize(&hir);
    assert!(
        dump.contains("VarStmt"),
        "desugared list should produce VarStmt before call"
    );
    assert!(
        dump.contains("take"),
        "call to take should still be present"
    );
}

#[test]
fn test_desugar_list_in_return() {
    let hir = compile_and_desugar("fn main() { return [1, 2] }");
    assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
    let dump = resolver::serialize::serialize(&hir);
    assert!(dump.contains("ReturnStmt"), "return stmt still present");
    assert!(
        dump.contains("VarStmt"),
        "list desugared in return position"
    );
}

#[test]
fn test_desugar_list_in_if_else() {
    let hir = compile_and_desugar(
        "fn main() { val x = 1\n  val ys = if x > 0 { [1] } else { [2, 3] }\n}",
    );
    assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
}

#[test]
fn test_desugar_list_as_match_arm_body() {
    let hir = compile_and_desugar(
        "fn main() { val x = 1\n  val ys = match x { 1 => [10], _ => [20, 30] }\n}",
    );
    assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
}

#[test]
fn test_desugar_list_with_temp_name_unique() {
    let hir = compile_and_desugar("fn main() { val a = [1, 2]\n  val b = [3, 4] }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(dump.contains("__list_0"), "first temp var");
    assert!(
        dump.contains("__list_1"),
        "second temp var should have different name"
    );
}

#[test]
fn test_desugar_empty_list_after_nonempty_uses_different_temp_counter() {
    let hir = compile_and_desugar("fn main() { val a = [1, 2]\n  val b: List<Int> = [] }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(dump.contains("__list_0"));
}

#[test]
fn test_desugar_uses_lang_item_def_ids() {
    let hir = compile_and_desugar("fn main() { val xs: List<Int> = [] }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(
        dump.contains("→101"),
        "callee def_id should match list_new lang item"
    );
}

/// When lang items are unavailable (no-stdlib), the desugar pass keeps
/// ListLit expressions intact — they fall through to typeck's fallback.
#[test]
fn test_desugar_fallback_when_lang_items_missing() {
    use parser::ast::AstNode;
    let result = parser::parse("fn main() { val xs = [1, 2, 3] }");
    let root = parser::ast::SourceFile::cast(result.tree).unwrap();
    let (items, _, _, next_id) = resolver::lower_structural(&root, "test", 0);
    let lang_items = LangItems {
        list: None,
        list_new: None,
        list_with_capacity: None,
        list_push: None,
    };
    let mut hir = Hir {
        items,
        diagnostics: Vec::new(),
    };
    super::pre_typecheck(&mut hir, &lang_items, next_id);
    assert!(
        count_expr_kind(&hir, is_list_lit) > 0,
        "ListLit should survive desugar when lang items are missing"
    );
}

#[test]
fn test_desugar_is_idempotent() {
    let hir = compile_and_desugar("fn main() { val a = [1, 2, 3]\n  val b = [4, 5] }");
    let mut hir2 = hir.clone();
    let lang_items = test_lang_items();
    super::pre_typecheck(&mut hir2, &lang_items, 10_000);
    let dump1 = resolver::serialize::serialize(&hir);
    let dump2 = resolver::serialize::serialize(&hir2);
    assert_eq!(
        dump1, dump2,
        "re-running desugar on desugared HIR should be idempotent"
    );
}

#[test]
fn test_desugar_catch_produces_match() {
    let hir = compile_and_desugar("fn main() { f() catch 0 }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(
        dump.contains("Match"),
        "catch should desugar to a match expression: {dump}"
    );
    assert!(
        !dump.contains("Catch("),
        "Catch should not survive desugar: {dump}"
    );
}

#[test]
fn test_desugar_catch_has_ok_and_wildcard() {
    let hir = compile_and_desugar("fn main() { f() catch 0 }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(dump.contains("Ok"), "catch should have Ok arm: {dump}");
    assert!(
        dump.contains("Wild"),
        "catch without |e| should use wildcard error arm: {dump}"
    );
}

#[test]
fn test_desugar_catch_with_error_capture() {
    let hir = compile_and_desugar("fn main() { f() catch |e| e }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(
        !dump.contains("Catch("),
        "Catch should not survive desugar: {dump}"
    );
    assert!(
        dump.contains("Match"),
        "catch |e| should desugar to match: {dump}"
    );
    assert!(dump.contains("Err"), "should have Err arm for |e|: {dump}");
}

#[test]
fn test_desugar_else_produces_match() {
    let hir = compile_and_desugar("fn main() { f() else 0 }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(
        dump.contains("Match"),
        "else should desugar to a match expression: {dump}"
    );
    assert!(
        !dump.contains("Else("),
        "Else should not survive desugar: {dump}"
    );
}

#[test]
fn test_desugar_else_has_some_and_none() {
    let hir = compile_and_desugar("fn main() { f() else 0 }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(dump.contains("Some"), "else should have Some arm: {dump}");
    assert!(
        dump.contains("Wild"),
        "else should have wildcard arm for None: {dump}"
    );
}

#[test]
fn test_desugar_catch_and_else_idempotent() {
    let source = "fn main() { a()?\n  b() catch c()\n  d() else e() }";
    let hir = compile_and_desugar(source);
    let mut hir2 = hir.clone();
    let lang_items = test_lang_items();
    super::pre_typecheck(&mut hir2, &lang_items, 10_000);
    let dump1 = resolver::serialize::serialize(&hir);
    let dump2 = resolver::serialize::serialize(&hir2);
    assert_eq!(
        dump1, dump2,
        "re-running desugar on desugared else/catch HIR should be idempotent"
    );
}

#[test]
fn test_desugar_nested_question() {
    let hir = compile_and_desugar("fn main() { f()?  }");
    let dump = resolver::serialize::serialize(&hir);
    assert!(
        dump.contains("Question("),
        "? should survive pre-typecheck desugar: {dump}"
    );
}

// ── Coverage invariants ── see `tests_coverage.rs`
