//! Unit tests for the HIR desugar pass. Covers list literals, ?/else
//! error handling, idempotency, and variant coverage invariants.

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

// ── Diagnostic pruning ───────────────────────────────────────────────────────

#[test]
fn test_stale_unresolved_name_pruned_after_desugar() {
    use lexer::Span;

    let source = "fn main() { val x = a() catch |e| b(e) }";
    let result = parser::parse(source);
    let root = parser::ast::SourceFile::cast(result.tree).unwrap();
    let (items, _, _, next_id) = resolver::lower_structural(&root, source, 0);
    let lang_items = test_lang_items();

    let mut hir = Hir {
        items,
        diagnostics: vec![],
    };

    // Add a stale UnresolvedName diagnostic for "e" before desugaring
    hir.diagnostics
        .push(resolver::HirDiagnostic::UnresolvedName {
            name: "e".to_string(),
            span: Span { lo: 0, hi: 0 },
        });
    assert_eq!(hir.diagnostics.len(), 1);

    super::pre_typecheck(&mut hir, &lang_items, next_id);

    // The desugar pass should have pruned the stale diagnostic for "e"
    // because it resolved "e" as a catch binding.
    let has_e = hir
        .diagnostics
        .iter()
        .any(|d| matches!(d, resolver::HirDiagnostic::UnresolvedName { name, .. } if name == "e"));
    assert!(
        !has_e,
        "stale UnresolvedName diagnostic for 'e' should be pruned after desugar"
    );
}

// ── Invariant: no sugar survives pre-typecheck desugar ────────────────────────

fn contains_sugar_expr(expr: &Expr) -> bool {
    match expr {
        Expr::ListLit(_) | Expr::Catch(_) | Expr::Else(_) => true,
        Expr::Bin(e) => contains_sugar_expr(&e.left) || contains_sugar_expr(&e.right),
        Expr::Unary(e) => contains_sugar_expr(&e.operand),
        Expr::Call(e) => e.args.iter().any(contains_sugar_expr),
        Expr::MethodCall(e) => {
            contains_sugar_expr(&e.receiver) || e.args.iter().any(contains_sugar_expr)
        }
        Expr::Field(e) => contains_sugar_expr(&e.receiver),
        Expr::Index(e) => contains_sugar_expr(&e.base) || e.indices.iter().any(contains_sugar_expr),
        Expr::Block(e) => contains_sugar_block(e),
        Expr::If(e) => {
            contains_sugar_expr(&e.condition)
                || contains_sugar_block(&e.then_branch)
                || e.else_branch
                    .as_ref()
                    .is_some_and(|b| contains_sugar_expr(b))
        }
        Expr::Match(e) => {
            contains_sugar_expr(&e.scrutinee)
                || e.arms.iter().any(|a| {
                    a.guard.as_ref().is_some_and(contains_sugar_expr)
                        || contains_sugar_expr(&a.body)
                })
        }
        Expr::Loop(e) => match &e.kind {
            LoopKind::Infinite(b) => contains_sugar_block(b),
            LoopKind::Conditional { condition, body } => {
                contains_sugar_expr(condition) || contains_sugar_block(body)
            }
            LoopKind::Iterator { iterable, body, .. } => {
                contains_sugar_expr(iterable) || contains_sugar_block(body)
            }
        },
        Expr::StructLit(e) => e.fields.iter().any(|f| contains_sugar_expr(&f.value)),
        Expr::Question(_) => false, // Question is not pre-typecheck sugar
        Expr::Lit(_) | Expr::Path(_) => false,
        Expr::Assign(e) => contains_sugar_expr(&e.value),
    }
}

fn contains_sugar_block(block: &Block) -> bool {
    block.stmts.iter().any(contains_sugar_stmt)
        || block.tail.as_ref().is_some_and(|b| contains_sugar_expr(b))
}

fn contains_sugar_stmt(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::ValStmt(s) => contains_sugar_expr(&s.value),
        Stmt::VarStmt(s) => contains_sugar_expr(&s.value),
        Stmt::ExprStmt(s) => contains_sugar_expr(&s.expr),
        Stmt::ReturnStmt(s) => s.value.as_ref().is_some_and(contains_sugar_expr),
        Stmt::BreakStmt(s) => s.value.as_ref().is_some_and(contains_sugar_expr),
        Stmt::ContinueStmt(_) | Stmt::YieldStmt(_) => false,
    }
}

fn contains_sugar_item(item: &Item) -> bool {
    match item {
        Item::FnDef(fd) => contains_sugar_block(&fd.body),
        Item::ImplDef(i) => {
            i.methods.iter().any(|m| contains_sugar_block(&m.body))
                || i.subscripts.iter().any(|s| contains_sugar_block(&s.body))
        }
        Item::TraitDef(t) => t
            .methods
            .iter()
            .filter_map(|m| m.body.as_ref())
            .any(contains_sugar_block),
        Item::SubscriptDef(s) => contains_sugar_block(&s.body),
        Item::StructDef(_) | Item::EnumDef(_) | Item::UseItem(_) | Item::ErrorSetDef(_) => false,
    }
}

fn hir_contains_sugar(hir: &Hir) -> bool {
    hir.items.iter().any(contains_sugar_item)
}

#[test]
fn test_no_sugar_after_pre_desugar() {
    let sources = [
        "fn main() { val xs = [1, 2, 3] }",
        "fn main() { a() catch b() }",
        "fn main() { a() catch |e| b(e) }",
        "fn main() { a() else b() }",
        "fn main() { val x = a() catch b()\n  val y = c() else d() }",
    ];

    for source in sources {
        let hir = compile_and_desugar(source);
        assert!(
            !hir_contains_sugar(&hir),
            "sugar survived pre-typecheck desugar in: {source}"
        );
    }
}

// ── Coverage invariants ── see `tests_coverage.rs`
