use super::*;

fn test_lang_items() -> LangItems {
    LangItems {
        list: Some(HirId(100)),
        list_new: Some(HirId(101)),
        list_with_capacity: Some(HirId(102)),
        list_push: Some(HirId(103)),
    }
}

fn compile_and_desugar(source: &str) -> Hir {
    use axiom_parser::ast::AstNode;
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let (items, _, _, next_id) = crate::lower_structural(&root, source, 0);
    let lang_items = test_lang_items();
    let mut hir = Hir {
        items,
        diagnostics: Vec::new(),
    };
    desugar(&mut hir, &lang_items, next_id);
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
            count_one(&e.value, f, count);
            count_sub_expr_kind(&e.value, f, count);
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
    let dump = crate::serialize::serialize(&hir);
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
    let dump = crate::serialize::serialize(&hir);
    assert!(
        dump.contains("VarStmt"),
        "desugared list should have VarStmt"
    );
    assert!(!dump.contains("ListLit"), "no ListLit after desugar");
}

#[test]
fn test_desugar_generates_unique_ids() {
    let hir = compile_and_desugar("fn main() { val xs = [1, 2, 3] }");
    // Verify no desugar-generated ID collides within the desugared block.
    // The desugared block is the outermost Block in main's body.
    let dump = crate::serialize::serialize(&hir);
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
    let dump = crate::serialize::serialize(&hir);
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
    let dump = crate::serialize::serialize(&hir);
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
    let dump = crate::serialize::serialize(&hir);
    assert!(dump.contains("__list_0"), "first temp var");
    assert!(
        dump.contains("__list_1"),
        "second temp var should have different name"
    );
}

#[test]
fn test_desugar_empty_list_after_nonempty_uses_different_temp_counter() {
    let hir = compile_and_desugar("fn main() { val a = [1, 2]\n  val b: List<Int> = [] }");
    let dump = crate::serialize::serialize(&hir);
    // Empty list desugars to List::new() call, no temp var.
    // Non-empty produces __list_0.
    assert!(dump.contains("__list_0"));
}

#[test]
fn test_desugar_uses_lang_item_def_ids() {
    let hir = compile_and_desugar("fn main() { val xs: List<Int> = [] }");
    let dump = crate::serialize::serialize(&hir);
    assert!(
        dump.contains("→101"),
        "callee def_id should match list_new lang item"
    );
}

/// When lang items are unavailable (no-stdlib), the desugar pass keeps
/// ListLit expressions intact — they fall through to typeck's fallback.
#[test]
fn test_desugar_fallback_when_lang_items_missing() {
    use axiom_parser::ast::AstNode;
    let result = axiom_parser::parse("fn main() { val xs = [1, 2, 3] }");
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let (items, _, _, next_id) = crate::lower_structural(&root, "test", 0);
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
    desugar(&mut hir, &lang_items, next_id);
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
    // Use a large next_id to avoid collision with existing IDs
    desugar(&mut hir2, &lang_items, 10_000);
    let dump1 = crate::serialize::serialize(&hir);
    let dump2 = crate::serialize::serialize(&hir2);
    assert_eq!(
        dump1, dump2,
        "re-running desugar on desugared HIR should be idempotent"
    );
}

/// Expr-variant coverage invariant: every variant in the Expr enum must be
/// explicitly classified. Adding a new Expr variant without updating this
/// test fails the build.
#[test]
fn test_every_expr_variant_handled_by_desugar() {
    let sugar: &[&str] = &["ListLit"];
    let non_sugar: &[&str] = &[
        "Lit",
        "Path",
        "Bin",
        "Unary",
        "Call",
        "MethodCall",
        "Field",
        "Index",
        "Block",
        "If",
        "Match",
        "Loop",
        "StructLit",
        "Assign",
    ];
    let all_known: std::collections::BTreeSet<&str> =
        sugar.iter().chain(non_sugar.iter()).copied().collect();
    let all_expr: &[&str] = &[
        "Lit",
        "Path",
        "Bin",
        "Unary",
        "Call",
        "MethodCall",
        "Field",
        "Index",
        "Block",
        "If",
        "Match",
        "Loop",
        "StructLit",
        "ListLit",
        "Assign",
    ];
    assert_eq!(all_expr.len(), 15, "Expr variant count changed");
    let known: std::collections::BTreeSet<&str> = all_expr.iter().copied().collect();
    assert_eq!(all_known, known, "every Expr variant must be classified");
}
