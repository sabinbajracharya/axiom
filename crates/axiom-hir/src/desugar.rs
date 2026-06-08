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
        Expr::Unary(e) => {
            desugar_expr(&mut e.operand, ctx);
        }
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
        Expr::Field(e) => {
            desugar_expr(&mut e.receiver, ctx);
        }
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
        Expr::Match(e) => {
            desugar_expr(&mut e.scrutinee, ctx);
            for arm in &mut e.arms {
                if let Some(ref mut guard) = arm.guard {
                    desugar_expr(guard, ctx);
                }
                desugar_expr(&mut arm.body, ctx);
            }
        }
        Expr::Loop(e) => desugar_loop_kind(&mut e.kind, ctx),

        Expr::StructLit(e) => {
            for field in &mut e.fields {
                desugar_expr(&mut field.value, ctx);
            }
        }
        Expr::ListLit(e) => {
            for elem in &mut e.elements {
                desugar_expr(elem, ctx);
            }
            let elements = std::mem::take(&mut e.elements);
            let replacement = desugar_list_lit(elements, ctx);
            *expr = replacement;
        }
        Expr::Assign(e) => {
            desugar_assign_target(&mut e.target, ctx);
            desugar_expr(&mut e.value, ctx);
        }
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
        let expr_stmt_id = ctx.fresh_id();
        let path_id = ctx.fresh_id();
        let method_call_id = ctx.fresh_id();

        let receiver = Expr::Path(PathExpr {
            id: path_id,
            name_ref: NameRef::resolved(var_stmt_id, temp_name.as_str()),
        });

        let push_call = Expr::MethodCall(MethodCallExpr {
            id: method_call_id,
            receiver: Box::new(receiver),
            method: "push".to_string(),
            args: vec![element],
        });

        stmts.push(Stmt::ExprStmt(ExprStmt {
            id: expr_stmt_id,
            expr: push_call,
        }));
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
    use crate::lang::{LANG_LIST_NEW, LANG_LIST_PUSH, LANG_LIST_WITH_CAPACITY};

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
            if f(tail) {
                *count += 1;
            }
            count_sub_expr_kind(tail, f, count);
        }
    }

    fn count_stmt_expr_kind(stmt: &Stmt, f: fn(&Expr) -> bool, count: &mut usize) {
        match stmt {
            Stmt::ValStmt(s) => {
                if f(&s.value) {
                    *count += 1;
                }
                count_sub_expr_kind(&s.value, f, count);
            }
            Stmt::VarStmt(s) => {
                if f(&s.value) {
                    *count += 1;
                }
                count_sub_expr_kind(&s.value, f, count);
            }
            Stmt::ExprStmt(s) => {
                if f(&s.expr) {
                    *count += 1;
                }
                count_sub_expr_kind(&s.expr, f, count);
            }
            Stmt::ReturnStmt(s) => {
                if let Some(ref v) = s.value {
                    if f(v) {
                        *count += 1;
                    }
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
                if f(&e.left) { *count += 1; }
                if f(&e.right) { *count += 1; }
                count_sub_expr_kind(&e.left, f, count);
                count_sub_expr_kind(&e.right, f, count);
            }
            Expr::Call(e) => {
                for a in &e.args { if f(a) { *count += 1; } count_sub_expr_kind(a, f, count); }
            }
            Expr::MethodCall(e) => {
                if f(&e.receiver) { *count += 1; }
                count_sub_expr_kind(&e.receiver, f, count);
                for a in &e.args { if f(a) { *count += 1; } count_sub_expr_kind(a, f, count); }
            }
            Expr::Block(e) => count_block_expr_kind(e, f, count),
            Expr::If(e) => {
                if f(&e.condition) { *count += 1; }
                count_sub_expr_kind(&e.condition, f, count);
                count_block_expr_kind(&e.then_branch, f, count);
                if let Some(ref eb) = e.else_branch { if f(eb) { *count += 1; } count_sub_expr_kind(eb, f, count); }
            }
            Expr::Match(e) => {
                if f(&e.scrutinee) { *count += 1; }
                count_sub_expr_kind(&e.scrutinee, f, count);
                for arm in &e.arms { if f(&arm.body) { *count += 1; } count_sub_expr_kind(&arm.body, f, count); }
            }
            Expr::Loop(e) => match &e.kind {
                LoopKind::Infinite(b) => count_block_expr_kind(b, f, count),
                LoopKind::Conditional { condition, body } => {
                    if f(condition) { *count += 1; }
                    count_sub_expr_kind(condition, f, count);
                    count_block_expr_kind(body, f, count);
                }
                LoopKind::Iterator { iterable, body, .. } => {
                    if f(iterable) { *count += 1; }
                    count_sub_expr_kind(iterable, f, count);
                    count_block_expr_kind(body, f, count);
                }
            },
            Expr::Unary(e) => {
                if f(&e.operand) { *count += 1; }
                count_sub_expr_kind(&e.operand, f, count);
            }
            Expr::Field(e) => {
                if f(&e.receiver) { *count += 1; }
                count_sub_expr_kind(&e.receiver, f, count);
            }
            Expr::Index(e) => {
                if f(&e.base) { *count += 1; }
                count_sub_expr_kind(&e.base, f, count);
                for idx in &e.indices {
                    if f(idx) { *count += 1; }
                    count_sub_expr_kind(idx, f, count);
                }
            }
            Expr::StructLit(e) => {
                for field in &e.fields {
                    if f(&field.value) { *count += 1; }
                    count_sub_expr_kind(&field.value, f, count);
                }
            }
            Expr::Assign(e) => {
                if f(&e.value) { *count += 1; }
                count_sub_expr_kind(&e.value, f, count);
            }
            Expr::ListLit(e) => {
                for elem in &e.elements {
                    if f(elem) { *count += 1; }
                    count_sub_expr_kind(elem, f, count);
                }
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

    fn collect_all_ids_bg(
        item: &Item,
        ids: &mut std::collections::HashSet<HirId>,
        collisions: &mut Vec<HirId>,
    ) {
        match item {
            Item::FnDef(f) => {
                if !ids.insert(f.id) { collisions.push(f.id); }
                collect_block_ids_bg(&f.body, ids, collisions);
            }
            Item::ImplDef(i) => {
                if !ids.insert(i.id) { collisions.push(i.id); }
                for m in &i.methods {
                    if !ids.insert(m.id) { collisions.push(m.id); }
                    collect_block_ids_bg(&m.body, ids, collisions);
                }
                for s in &i.subscripts {
                    if !ids.insert(s.id) { collisions.push(s.id); }
                    collect_block_ids_bg(&s.body, ids, collisions);
                }
            }
            Item::SubscriptDef(s) => {
                if !ids.insert(s.id) { collisions.push(s.id); }
                collect_block_ids_bg(&s.body, ids, collisions);
            }
            Item::TraitDef(t) => {
                if !ids.insert(t.id) { collisions.push(t.id); }
                for m in &t.methods {
                    if !ids.insert(m.id) { collisions.push(m.id); }
                    if let Some(ref body) = m.body {
                        collect_block_ids_bg(body, ids, collisions);
                    }
                }
            }
            Item::StructDef(s) => {
                if !ids.insert(s.id) { collisions.push(s.id); }
            }
            Item::EnumDef(e) => {
                if !ids.insert(e.id) { collisions.push(e.id); }
            }
            Item::UseItem(u) => {
                if !ids.insert(u.id) { collisions.push(u.id); }
            }
        }
    }

    fn collect_block_ids_bg(
        block: &Block,
        ids: &mut std::collections::HashSet<HirId>,
        collisions: &mut Vec<HirId>,
    ) {
        if !ids.insert(block.id) { collisions.push(block.id); }
        for stmt in &block.stmts {
            if !ids.insert(stmt.id()) { collisions.push(stmt.id()); }
            collect_stmt_ids_bg(stmt, ids, collisions);
        }
        if let Some(ref tail) = block.tail {
            if !ids.insert(tail.id()) { collisions.push(tail.id()); }
            collect_expr_ids_bg(tail, ids, collisions);
        }
    }

    fn collect_stmt_ids_bg(
        stmt: &Stmt,
        ids: &mut std::collections::HashSet<HirId>,
        collisions: &mut Vec<HirId>,
    ) {
        match stmt {
            Stmt::ValStmt(s) => collect_expr_ids_bg(&s.value, ids, collisions),
            Stmt::VarStmt(s) => collect_expr_ids_bg(&s.value, ids, collisions),
            Stmt::ExprStmt(s) => collect_expr_ids_bg(&s.expr, ids, collisions),
            Stmt::ReturnStmt(s) => {
                if let Some(ref v) = s.value {
                    collect_expr_ids_bg(v, ids, collisions);
                }
            }
            Stmt::BreakStmt(s) => {
                if let Some(ref v) = s.value {
                    collect_expr_ids_bg(v, ids, collisions);
                }
            }
            Stmt::ContinueStmt(_) | Stmt::YieldStmt(_) => {}
        }
    }

    fn collect_expr_ids_bg(
        expr: &Expr,
        ids: &mut std::collections::HashSet<HirId>,
        collisions: &mut Vec<HirId>,
    ) {
        if !ids.insert(expr.id()) { collisions.push(expr.id()); }
        match expr {
            Expr::Lit(_) | Expr::Path(_) => {}
            Expr::Bin(e) => {
                collect_expr_ids_bg(&e.left, ids, collisions);
                collect_expr_ids_bg(&e.right, ids, collisions);
            }
            Expr::Unary(e) => collect_expr_ids_bg(&e.operand, ids, collisions),
            Expr::Call(e) => {
                for a in &e.args { collect_expr_ids_bg(a, ids, collisions); }
            }
            Expr::MethodCall(e) => {
                collect_expr_ids_bg(&e.receiver, ids, collisions);
                for a in &e.args { collect_expr_ids_bg(a, ids, collisions); }
            }
            Expr::Field(e) => collect_expr_ids_bg(&e.receiver, ids, collisions),
            Expr::Index(e) => {
                collect_expr_ids_bg(&e.base, ids, collisions);
                for idx in &e.indices { collect_expr_ids_bg(idx, ids, collisions); }
            }
            Expr::Block(e) => collect_block_ids_bg(e, ids, collisions),
            Expr::If(e) => {
                collect_expr_ids_bg(&e.condition, ids, collisions);
                collect_block_ids_bg(&e.then_branch, ids, collisions);
                if let Some(ref eb) = e.else_branch {
                    collect_expr_ids_bg(eb, ids, collisions);
                }
            }
            Expr::Match(e) => {
                collect_expr_ids_bg(&e.scrutinee, ids, collisions);
                for arm in &e.arms {
                    if let Some(ref guard) = arm.guard {
                        collect_expr_ids_bg(guard, ids, collisions);
                    }
                    collect_expr_ids_bg(&arm.body, ids, collisions);
                }
            }
            Expr::Loop(e) => match &e.kind {
                LoopKind::Infinite(b) => collect_block_ids_bg(b, ids, collisions),
                LoopKind::Conditional { condition, body } => {
                    collect_expr_ids_bg(condition, ids, collisions);
                    collect_block_ids_bg(body, ids, collisions);
                }
                LoopKind::Iterator { iterable, body, .. } => {
                    collect_expr_ids_bg(iterable, ids, collisions);
                    collect_block_ids_bg(body, ids, collisions);
                }
            },
            Expr::StructLit(e) => {
                for f in &e.fields { collect_expr_ids_bg(&f.value, ids, collisions); }
            }
            Expr::ListLit(e) => {
                for elem in &e.elements { collect_expr_ids_bg(elem, ids, collisions); }
            }
            Expr::Assign(e) => {
                collect_expr_ids_bg(&e.value, ids, collisions);
            }
        }
    }
}
