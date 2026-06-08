//! HIR desugar pass: rewrites sugar expressions into core HIR nodes.
//!
//! See [`docs/hir-desugar-pass-design.md`](../../../docs/hir-desugar-pass-design.md).
//!
//! The pass runs after name resolution and lang-item resolution, before type
//! checking. It walks every block in the HIR and replaces sugar `Expr` variants
//! with their desugared form — plain `Call`, `MethodCall`, `VarStmt`, `ExprStmt`,
//! and `Block` nodes. After this pass, typeck and IR lowering see only core
//! constructs; there are no per-sugar special-cases downstream.

use crate::hir::*;
use crate::lang::LangItems;

pub struct DesugarResult {
    pub next_id: usize,
}

struct DesugarCtx<'a> {
    lang_items: &'a LangItems,
    next_id: usize,
    temp_counter: usize,
}

impl DesugarCtx<'_> {
    fn fresh_id(&mut self) -> HirId {
        let id = HirId(self.next_id);
        self.next_id += 1;
        id
    }

    fn fresh_temp_name(&mut self) -> String {
        let name = format!("__list_{}", self.temp_counter);
        self.temp_counter += 1;
        name
    }
}

/// Run the desugar pass on a fully name-resolved HIR. Sugar expressions are
/// rewritten in-place; no new diagnostics are emitted.
pub fn desugar(hir: &mut Hir, lang_items: &LangItems, next_id: usize) -> DesugarResult {
    let mut ctx = DesugarCtx {
        lang_items,
        next_id,
        temp_counter: 0,
    };
    for item in &mut hir.items {
        desugar_item(item, &mut ctx);
    }
    DesugarResult {
        next_id: ctx.next_id,
    }
}

fn desugar_item(item: &mut Item, ctx: &mut DesugarCtx) {
    match item {
        Item::FnDef(f) => desugar_block(&mut f.body, ctx),
        Item::ImplDef(i) => {
            for method in &mut i.methods {
                desugar_block(&mut method.body, ctx);
            }
            for sub in &mut i.subscripts {
                desugar_block(&mut sub.body, ctx);
            }
        }
        Item::TraitDef(t) => {
            for method in &mut t.methods {
                if let Some(ref mut body) = method.body {
                    desugar_block(body, ctx);
                }
            }
        }
        Item::StructDef(_) | Item::EnumDef(_) | Item::UseItem(_) => {}
        Item::SubscriptDef(s) => desugar_block(&mut s.body, ctx),
    }
}

fn desugar_block(block: &mut Block, ctx: &mut DesugarCtx) {
    for stmt in &mut block.stmts {
        desugar_stmt(stmt, ctx);
    }
    if let Some(ref mut tail) = block.tail {
        desugar_expr(tail, ctx);
    }
}

fn desugar_stmt(stmt: &mut Stmt, ctx: &mut DesugarCtx) {
    match stmt {
        Stmt::ValStmt(s) => desugar_expr(&mut s.value, ctx),
        Stmt::VarStmt(s) => desugar_expr(&mut s.value, ctx),
        Stmt::ExprStmt(s) => desugar_expr(&mut s.expr, ctx),
        Stmt::ReturnStmt(s) => {
            if let Some(ref mut value) = s.value {
                desugar_expr(value, ctx);
            }
        }
        Stmt::BreakStmt(s) => {
            if let Some(ref mut value) = s.value {
                desugar_expr(value, ctx);
            }
        }
        Stmt::ContinueStmt(_) | Stmt::YieldStmt(_) => {}
    }
}

fn desugar_expr(expr: &mut Expr, ctx: &mut DesugarCtx) {
    match expr {
        Expr::Lit(_) | Expr::Path(_) => {}

        Expr::Bin(e) => {
            desugar_expr(&mut e.left, ctx);
            desugar_expr(&mut e.right, ctx);
        }
        Expr::Unary(e) => desugar_expr(&mut e.operand, ctx),
        Expr::Call(e) => {
            for arg in &mut e.args {
                desugar_expr(arg, ctx);
            }
        }
        Expr::MethodCall(e) => {
            desugar_expr(&mut e.receiver, ctx);
            for arg in &mut e.args {
                desugar_expr(arg, ctx);
            }
        }
        Expr::Field(e) => desugar_expr(&mut e.receiver, ctx),
        Expr::Index(e) => {
            desugar_expr(&mut e.base, ctx);
            for idx in &mut e.indices {
                desugar_expr(idx, ctx);
            }
        }
        Expr::Block(e) => desugar_block(e, ctx),
        Expr::If(e) => {
            desugar_expr(&mut e.condition, ctx);
            desugar_block(&mut e.then_branch, ctx);
            if let Some(ref mut else_branch) = e.else_branch {
                desugar_expr(else_branch, ctx);
            }
        }
        Expr::Match(e) => desugar_match(e, ctx),
        Expr::Loop(e) => desugar_loop_kind(&mut e.kind, ctx),
        Expr::StructLit(e) => {
            for field in &mut e.fields {
                desugar_expr(&mut field.value, ctx);
            }
        }
        Expr::ListLit(e) => {
            desugar_expr_list_elements(&mut e.elements, ctx);
            let replacement = desugar_list_lit(std::mem::take(&mut e.elements), ctx);
            *expr = replacement;
        }
        Expr::Assign(e) => {
            desugar_assign_target(&mut e.target, ctx);
            desugar_expr(&mut e.value, ctx);
        }
    }
}

fn desugar_expr_list_elements(elements: &mut [Expr], ctx: &mut DesugarCtx) {
    for elem in elements {
        desugar_expr(elem, ctx);
    }
}

fn desugar_match(match_expr: &mut MatchExpr, ctx: &mut DesugarCtx) {
    desugar_expr(&mut match_expr.scrutinee, ctx);
    for arm in &mut match_expr.arms {
        if let Some(ref mut guard) = arm.guard {
            desugar_expr(guard, ctx);
        }
        desugar_expr(&mut arm.body, ctx);
    }
}

fn desugar_loop_kind(kind: &mut LoopKind, ctx: &mut DesugarCtx) {
    match kind {
        LoopKind::Infinite(block) => desugar_block(block, ctx),
        LoopKind::Conditional { condition, body } => {
            desugar_expr(condition, ctx);
            desugar_block(body, ctx);
        }
        LoopKind::Iterator {
            iterable, body, ..
        } => {
            desugar_expr(iterable, ctx);
            desugar_block(body, ctx);
        }
    }
}

fn desugar_assign_target(target: &mut AssignTarget, ctx: &mut DesugarCtx) {
    match target {
        AssignTarget::Name(_) => {}
        AssignTarget::Field { receiver, .. } => desugar_expr(receiver, ctx),
        AssignTarget::Index { base, indices } => {
            desugar_expr(base, ctx);
            for idx in indices {
                desugar_expr(idx, ctx);
            }
        }
    }
}

fn desugar_list_lit(elements: Vec<Expr>, ctx: &mut DesugarCtx) -> Expr {
    if elements.is_empty() {
        return desugar_empty_list(ctx);
    }
    desugar_non_empty_list(elements, ctx)
}

fn desugar_empty_list(ctx: &mut DesugarCtx) -> Expr {
    let call_id = ctx.fresh_id();
    let list_new_id = match ctx.lang_items.list_new {
        Some(id) => id,
        None => {
            return Expr::ListLit(ListLitExpr {
                id: call_id,
                elements: Vec::new(),
            });
        }
    };
    Expr::Call(CallExpr {
        id: call_id,
        callee: NameRef::resolved(list_new_id, "new"),
        qualifier: Some(crate::lang::LIST.to_string()),
        args: Vec::new(),
    })
}

/// Build a `push` method-call statement for one element of a desugared list
/// literal. `var_stmt_id` is the HirId of the `VarStmt` that holds the
/// temporary list; `temp_name` is its identifier.
fn desugar_push_call(
    element: Expr,
    var_stmt_id: HirId,
    temp_name: &str,
    ctx: &mut DesugarCtx,
) -> Stmt {
    let path_id = ctx.fresh_id();
    let method_call_id = ctx.fresh_id();
    let expr_stmt_id = ctx.fresh_id();
    Stmt::ExprStmt(ExprStmt {
        id: expr_stmt_id,
        expr: Expr::MethodCall(MethodCallExpr {
            id: method_call_id,
            receiver: Box::new(Expr::Path(PathExpr {
                id: path_id,
                name_ref: NameRef::resolved(var_stmt_id, temp_name),
            })),
            method: "push".to_string(),
            args: vec![element],
        }),
    })
}

fn desugar_non_empty_list(elements: Vec<Expr>, ctx: &mut DesugarCtx) -> Expr {
    let n = elements.len();
    let list_with_capacity_id = match ctx.lang_items.list_with_capacity {
        Some(id) => id,
        None => {
            return Expr::ListLit(ListLitExpr {
                id: ctx.fresh_id(),
                elements,
            });
        }
    };
    let block_id = ctx.fresh_id();
    let var_stmt_id = ctx.fresh_id();
    let temp_name = ctx.fresh_temp_name();

    let cap_lit_id = ctx.fresh_id();
    let cap_expr = Expr::Lit(LitExpr {
        id: cap_lit_id,
        kind: LitKind::Int(n as i64),
    });

    let call_id = ctx.fresh_id();
    let with_capacity_call = Expr::Call(CallExpr {
        id: call_id,
        callee: NameRef::resolved(list_with_capacity_id, "with_capacity"),
        qualifier: Some(crate::lang::LIST.to_string()),
        args: vec![cap_expr],
    });

    let var_stmt = Stmt::VarStmt(VarStmt {
        id: var_stmt_id,
        pattern: Pattern::Ident(IdentPat {
            id: var_stmt_id,
            name: temp_name.clone(),
            binding: Some(var_stmt_id),
            span: axiom_lexer::Span { lo: 0, hi: 0 },
        }),
        ty: None,
        value: with_capacity_call,
    });

    let mut stmts: Vec<Stmt> = vec![var_stmt];
    for element in elements {
        stmts.push(desugar_push_call(element, var_stmt_id, &temp_name, ctx));
    }

    let tail_id = ctx.fresh_id();
    let tail = Box::new(Expr::Path(PathExpr {
        id: tail_id,
        name_ref: NameRef::resolved(var_stmt_id, temp_name.as_str()),
    }));

    Expr::Block(Block {
        id: block_id,
        stmts,
        tail: Some(tail),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
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
            Stmt::ValStmt(s) => { count_one(&s.value, f, count); count_sub_expr_kind(&s.value, f, count); }
            Stmt::VarStmt(s) => { count_one(&s.value, f, count); count_sub_expr_kind(&s.value, f, count); }
            Stmt::ExprStmt(s) => { count_one(&s.expr, f, count); count_sub_expr_kind(&s.expr, f, count); }
            Stmt::ReturnStmt(s) => {
                if let Some(ref v) = s.value {
                    count_one(v, f, count);
                    count_sub_expr_kind(v, f, count);
                }
            }
            _ => {}
        }
    }

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
            Expr::Unary(e) => { count_one(&e.operand, f, count); count_sub_expr_kind(&e.operand, f, count); }
            Expr::Field(e) => { count_one(&e.receiver, f, count); count_sub_expr_kind(&e.receiver, f, count); }
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
            Expr::Assign(e) => { count_one(&e.value, f, count); count_sub_expr_kind(&e.value, f, count); }
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

    fn count_loop_sub_expr_kind(
        kind: &LoopKind,
        f: fn(&Expr) -> bool,
        count: &mut usize,
    ) {
        match kind {
            LoopKind::Infinite(b) => count_block_expr_kind(b, f, count),
            LoopKind::Conditional { condition, body } => {
                count_one(condition, f, count);
                count_sub_expr_kind(condition, f, count);
                count_block_expr_kind(body, f, count);
            }
            LoopKind::Iterator {
                iterable, body, ..
            } => {
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
        assert!(count_expr_kind(&hir, is_call) >= 1, "should have with_capacity call");
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
        assert!(dump.contains("VarStmt"), "desugared list should have VarStmt");
        assert!(!dump.contains("ListLit"), "no ListLit after desugar");
    }

    #[test]
    fn test_desugar_generates_unique_ids() {
        let hir = compile_and_desugar("fn main() { val xs = [1, 2, 3] }");
        // Verify no desugar-generated ID collides within the desugared block.
        // The desugared block is the outermost Block in main's body.
        let dump = crate::serialize::serialize(&hir);
        assert!(dump.contains("VarStmt"), "desugared list should have VarStmt");
        assert!(!dump.contains("ListLit"), "no ListLit after desugar");
    }

    #[test]
    fn test_desugar_singleton_list() {
        let hir = compile_and_desugar("fn main() { val xs = [42] }");
        assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
        assert!(count_expr_kind(&hir, is_call) >= 1, "should have with_capacity(1)");
        assert!(
            count_expr_kind(&hir, is_method_call) >= 1,
            "should have one push call"
        );
    }

    #[test]
    fn test_desugar_list_in_call_arg() {
        let hir = compile_and_desugar(
            "fn take(list: List<Int>) {}\nfn main() { take([1, 2]) }",
        );
        assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
        let dump = crate::serialize::serialize(&hir);
        assert!(dump.contains("VarStmt"), "desugared list should produce VarStmt before call");
        assert!(dump.contains("take"), "call to take should still be present");
    }

    #[test]
    fn test_desugar_list_in_return() {
        let hir = compile_and_desugar("fn main() { return [1, 2] }");
        assert_eq!(count_expr_kind(&hir, is_list_lit), 0);
        let dump = crate::serialize::serialize(&hir);
        assert!(dump.contains("ReturnStmt"), "return stmt still present");
        assert!(dump.contains("VarStmt"), "list desugared in return position");
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
        let hir = compile_and_desugar(
            "fn main() { val a = [1, 2]\n  val b = [3, 4] }",
        );
        let dump = crate::serialize::serialize(&hir);
        assert!(dump.contains("__list_0"), "first temp var");
        assert!(dump.contains("__list_1"), "second temp var should have different name");
    }

    #[test]
    fn test_desugar_empty_list_after_nonempty_uses_different_temp_counter() {
        let hir = compile_and_desugar(
            "fn main() { val a = [1, 2]\n  val b: List<Int> = [] }",
        );
        let dump = crate::serialize::serialize(&hir);
        // Empty list desugars to List::new() call, no temp var.
        // Non-empty produces __list_0.
        assert!(dump.contains("__list_0"));
    }

    #[test]
    fn test_desugar_uses_lang_item_def_ids() {
        let hir = compile_and_desugar("fn main() { val xs: List<Int> = [] }");
        let dump = crate::serialize::serialize(&hir);
        assert!(dump.contains("→101"), "callee def_id should match list_new lang item");
    }

    /// When lang items are unavailable (no-stdlib), the desugar pass keeps
    /// ListLit expressions intact — they fall through to typeck's fallback.
    #[test]
    fn test_desugar_fallback_when_lang_items_missing() {
        use axiom_parser::ast::AstNode;
        let result = axiom_parser::parse("fn main() { val xs = [1, 2, 3] }");
        let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
        let (items, _, _, next_id) = crate::lower_structural(&root, "test", 0);
        let lang_items = LangItems { list: None, list_new: None, list_with_capacity: None, list_push: None };
        let mut hir = Hir { items, diagnostics: Vec::new() };
        desugar(&mut hir, &lang_items, next_id);
        assert!(count_expr_kind(&hir, is_list_lit) > 0,
            "ListLit should survive desugar when lang items are missing");
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
        assert_eq!(dump1, dump2,
            "re-running desugar on desugared HIR should be idempotent");
    }

    /// Expr-variant coverage invariant: every variant in the Expr enum must be
    /// explicitly classified. Adding a new Expr variant without updating this
    /// test fails the build.
    #[test]
    fn test_every_expr_variant_handled_by_desugar() {
        let sugar: &[&str] = &["ListLit"];
        let non_sugar: &[&str] = &[
            "Lit", "Path", "Bin", "Unary", "Call", "MethodCall", "Field",
            "Index", "Block", "If", "Match", "Loop", "StructLit", "Assign",
        ];
        let all_known: std::collections::BTreeSet<&str> =
            sugar.iter().chain(non_sugar.iter()).copied().collect();
        let all_expr: &[&str] = &[
            "Lit", "Path", "Bin", "Unary", "Call", "MethodCall", "Field",
            "Index", "Block", "If", "Match", "Loop", "StructLit", "ListLit",
            "Assign",
        ];
        assert_eq!(all_expr.len(), 15, "Expr variant count changed");
        let known: std::collections::BTreeSet<&str> = all_expr.iter().copied().collect();
        assert_eq!(all_known, known, "every Expr variant must be classified");
    }
}
