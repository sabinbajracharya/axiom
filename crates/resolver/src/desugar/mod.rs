//! HIR desugar pass: rewrites sugar expressions into core HIR nodes.
//!
//! See [`docs/hir-desugar-pass-design.md`](../../../docs/hir-desugar-pass-design.md).
//!
//! The pass runs after name resolution and lang-item resolution, before type
//! checking. It walks every block in the HIR and replaces sugar `Expr` variants
//! with their desugared form — plain `Call`, `MethodCall`, `VarStmt`, `ExprStmt`,
//! and `Block` nodes. After this pass, typeck and IR lowering see only core
//! constructs; there are no per-sugar special-cases downstream.

use crate::hir_types::*;
use crate::lang::LangItems;
use crate::HirDiagnostic;

struct DesugarCtx<'a> {
    lang_items: &'a LangItems,
    next_id: usize,
    temp_counter: usize,
    /// Names that were resolved by the desugar pass (e.g. `catch |e|`
    /// capture variables). Used to clean up stale `UnresolvedName`
    /// diagnostics after desugaring.
    fixed_names: Vec<String>,
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
pub fn desugar(hir: &mut Hir, lang_items: &LangItems, next_id: usize) {
    let mut ctx = DesugarCtx {
        lang_items,
        next_id,
        temp_counter: 0,
        fixed_names: Vec::new(),
    };
    for item in &mut hir.items {
        desugar_item(item, &mut ctx);
    }
    // Remove stale UnresolvedName diagnostics for variables the desugar
    // pass resolved (e.g. `catch |e|` capture bindings).
    hir.diagnostics.retain(|d| {
        !matches!(d, HirDiagnostic::UnresolvedName { name, .. } if ctx.fixed_names.contains(name))
    });
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
        Item::StructDef(_) | Item::EnumDef(_) | Item::UseItem(_) | Item::ErrorSetDef(_) => {}
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
        Expr::Question(e) => {
            desugar_expr(&mut e.expr, ctx);
            // ? is NOT desugared pre-typecheck — it needs type info to determine
            // whether to generate Some/None (Option) or Ok/Err (Result) match arms.
            // The post-typecheck desugar pass (in the typecheck crate) handles it.
        }
        Expr::Catch(e) => {
            desugar_expr(&mut e.expr, ctx);
            desugar_expr(&mut e.fallback, ctx);
            *expr = desugar_catch(e, ctx);
        }
        Expr::Else(e) => {
            desugar_expr(&mut e.expr, ctx);
            desugar_expr(&mut e.fallback, ctx);
            *expr = desugar_else(e, ctx);
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
        LoopKind::Iterator { iterable, body, .. } => {
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

/// Build the `Ok(v) => v` success arm for catch desugaring.
fn build_result_ok_arm(binding_name: &str, ctx: &mut DesugarCtx) -> MatchArm {
    let ok_pat_id = ctx.fresh_id();
    let ok_binding_id = ctx.fresh_id();
    let ok_body_id = ctx.fresh_id();

    let ok_pat = Pattern::TupleStruct(TupleStructPat {
        id: ok_pat_id,
        path: NameRef::unresolved("Ok"),
        fields: vec![Pattern::Ident(IdentPat {
            id: ok_binding_id,
            name: binding_name.to_string(),
            binding: Some(ok_binding_id),
            span: lexer::Span { lo: 0, hi: 0 },
        })],
    });

    MatchArm {
        pattern: ok_pat,
        guard: None,
        body: Expr::Path(PathExpr {
            id: ok_body_id,
            name_ref: NameRef::resolved(ok_binding_id, binding_name),
        }),
    }
}

/// Desugar `expr catch fallback` → `match expr { Ok(v) => v, Err(_) => fallback }`
/// or `expr catch |e| handler` → `match expr { Ok(v) => v, Err(e) => handler }`.
fn desugar_catch(catch_expr: &CatchExpr, ctx: &mut DesugarCtx) -> Expr {
    let scrutinee = catch_expr.expr.clone();
    let mut fallback = catch_expr.fallback.clone();
    let match_id = ctx.fresh_id();

    let ok_binding_name = format!("__catch_ok_{}", ctx.temp_counter);
    ctx.temp_counter += 1;
    let ok_arm = build_result_ok_arm(&ok_binding_name, ctx);

    let err_arm = if let (Some(ref name), Some(binding_id)) =
        (&catch_expr.error_binding, catch_expr.error_binding_id)
    {
        let err_pat_id = ctx.fresh_id();
        let err_pat = Pattern::TupleStruct(TupleStructPat {
            id: err_pat_id,
            path: NameRef::unresolved("Err"),
            fields: vec![Pattern::Ident(IdentPat {
                id: binding_id,
                name: name.clone(),
                binding: Some(binding_id),
                span: lexer::Span { lo: 0, hi: 0 },
            })],
        });
        // Wire up body references: the lowered closure body contains
        // `NameRef::Unresolved(name)` references that were never resolved
        // (the closure param was destructured). Point them at the new
        // match-arm binding.
        replace_unresolved_name(&mut fallback, name, binding_id);
        ctx.fixed_names.push(name.clone());
        MatchArm {
            pattern: err_pat,
            guard: None,
            body: *fallback,
        }
    } else if let Some(ref name) = catch_expr.error_binding {
        // Legacy path: error_binding set but no ID (pre-HirId-tracking).
        // Use a fresh ID and hope the references get resolved downstream.
        let err_pat_id = ctx.fresh_id();
        let err_binding_id = ctx.fresh_id();
        let err_pat = Pattern::TupleStruct(TupleStructPat {
            id: err_pat_id,
            path: NameRef::unresolved("Err"),
            fields: vec![Pattern::Ident(IdentPat {
                id: err_binding_id,
                name: name.clone(),
                binding: Some(err_binding_id),
                span: lexer::Span { lo: 0, hi: 0 },
            })],
        });
        replace_unresolved_name(&mut fallback, name, err_binding_id);
        MatchArm {
            pattern: err_pat,
            guard: None,
            body: *fallback,
        }
    } else {
        MatchArm {
            pattern: Pattern::Wildcard(ctx.fresh_id()),
            guard: None,
            body: *fallback,
        }
    };

    Expr::Match(MatchExpr {
        id: match_id,
        scrutinee,
        arms: vec![ok_arm, err_arm],
    })
}

/// Desugar `expr else fallback` → `match expr { Some(v) => v, None => fallback }`.
fn desugar_else(else_expr: &ElseExpr, ctx: &mut DesugarCtx) -> Expr {
    let scrutinee = else_expr.expr.clone();
    let fallback = else_expr.fallback.clone();
    let match_id = ctx.fresh_id();

    let some_pat_id = ctx.fresh_id();
    let some_binding_id = ctx.fresh_id();
    let some_body_id = ctx.fresh_id();
    let some_binding_name = format!("__else_some_{}", ctx.temp_counter);
    ctx.temp_counter += 1;

    let some_pat = Pattern::TupleStruct(TupleStructPat {
        id: some_pat_id,
        path: NameRef::unresolved("Some"),
        fields: vec![Pattern::Ident(IdentPat {
            id: some_binding_id,
            name: some_binding_name.clone(),
            binding: Some(some_binding_id),
            span: lexer::Span { lo: 0, hi: 0 },
        })],
    });

    let some_arm = MatchArm {
        pattern: some_pat,
        guard: None,
        body: Expr::Path(PathExpr {
            id: some_body_id,
            name_ref: NameRef::resolved(some_binding_id, &some_binding_name),
        }),
    };

    let none_arm = MatchArm {
        pattern: Pattern::Wildcard(ctx.fresh_id()),
        guard: None,
        body: *fallback,
    };

    Expr::Match(MatchExpr {
        id: match_id,
        scrutinee,
        arms: vec![some_arm, none_arm],
    })
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
            span: lexer::Span { lo: 0, hi: 0 },
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

/// Walk an expression tree and replace every `NameRef::Unresolved(name)`
/// with `NameRef::Resolved(binding_id, name)`. Used by `catch |e|`
/// desugaring to wire the new match-arm binding to the body's references.
///
/// This function matches every `Expr` variant (enforced by
/// `test_every_expr_variant_handled_by_desugar`). Each arm recurses into
/// child expressions; there is no shorter correct implementation.
#[allow(clippy::too_many_lines)]
fn replace_unresolved_name(expr: &mut Expr, name: &str, binding_id: HirId) {
    match expr {
        Expr::Path(p) => {
            if let NameRef::Unresolved(ref u) = p.name_ref {
                if u.text == name {
                    p.name_ref = NameRef::resolved(binding_id, name);
                }
            }
        }
        Expr::Bin(e) => {
            replace_unresolved_name(&mut e.left, name, binding_id);
            replace_unresolved_name(&mut e.right, name, binding_id);
        }
        Expr::Unary(e) => {
            replace_unresolved_name(&mut e.operand, name, binding_id);
        }
        Expr::Call(e) => {
            for arg in &mut e.args {
                replace_unresolved_name(arg, name, binding_id);
            }
        }
        Expr::MethodCall(e) => {
            replace_unresolved_name(&mut e.receiver, name, binding_id);
            for arg in &mut e.args {
                replace_unresolved_name(arg, name, binding_id);
            }
        }
        Expr::Field(e) => {
            replace_unresolved_name(&mut e.receiver, name, binding_id);
        }
        Expr::Index(e) => {
            replace_unresolved_name(&mut e.base, name, binding_id);
            for idx in &mut e.indices {
                replace_unresolved_name(idx, name, binding_id);
            }
        }
        Expr::Block(e) => {
            for stmt in &mut e.stmts {
                replace_unresolved_name_in_stmt(stmt, name, binding_id);
            }
            if let Some(ref mut tail) = e.tail {
                replace_unresolved_name(tail, name, binding_id);
            }
        }
        Expr::If(e) => {
            replace_unresolved_name(&mut e.condition, name, binding_id);
            replace_unresolved_name_in_block(&mut e.then_branch, name, binding_id);
            if let Some(ref mut els) = e.else_branch {
                replace_unresolved_name(els, name, binding_id);
            }
        }
        Expr::Match(e) => {
            replace_unresolved_name(&mut e.scrutinee, name, binding_id);
            for arm in &mut e.arms {
                if let Some(ref mut guard) = arm.guard {
                    replace_unresolved_name(guard, name, binding_id);
                }
                replace_unresolved_name(&mut arm.body, name, binding_id);
            }
        }
        Expr::Loop(e) => match &mut e.kind {
            LoopKind::Infinite(b) => replace_unresolved_name_in_block(b, name, binding_id),
            LoopKind::Conditional { condition, body } => {
                replace_unresolved_name(condition, name, binding_id);
                replace_unresolved_name_in_block(body, name, binding_id);
            }
            LoopKind::Iterator { iterable, body, .. } => {
                replace_unresolved_name(iterable, name, binding_id);
                replace_unresolved_name_in_block(body, name, binding_id);
            }
        },
        Expr::StructLit(e) => {
            for field in &mut e.fields {
                replace_unresolved_name(&mut field.value, name, binding_id);
            }
        }
        Expr::Assign(e) => {
            replace_unresolved_name_in_assign_target(&mut e.target, name, binding_id);
            replace_unresolved_name(&mut e.value, name, binding_id);
        }
        Expr::Question(e) => {
            replace_unresolved_name(&mut e.expr, name, binding_id);
        }
        Expr::Catch(_) | Expr::Else(_) | Expr::ListLit(_) => {
            // Catch/Else are desugared; ListLit may fall through when lang items are missing.
        }
        Expr::Lit(_) => {}
    }
}

fn replace_unresolved_name_in_stmt(stmt: &mut Stmt, name: &str, binding_id: HirId) {
    match stmt {
        Stmt::ValStmt(s) => replace_unresolved_name(&mut s.value, name, binding_id),
        Stmt::VarStmt(s) => replace_unresolved_name(&mut s.value, name, binding_id),
        Stmt::ExprStmt(s) => replace_unresolved_name(&mut s.expr, name, binding_id),
        Stmt::ReturnStmt(s) => {
            if let Some(ref mut v) = s.value {
                replace_unresolved_name(v, name, binding_id);
            }
        }
        Stmt::BreakStmt(s) => {
            if let Some(ref mut v) = s.value {
                replace_unresolved_name(v, name, binding_id);
            }
        }
        _ => {}
    }
}

fn replace_unresolved_name_in_block(block: &mut Block, name: &str, binding_id: HirId) {
    for stmt in &mut block.stmts {
        replace_unresolved_name_in_stmt(stmt, name, binding_id);
    }
    if let Some(ref mut tail) = block.tail {
        replace_unresolved_name(tail, name, binding_id);
    }
}

fn replace_unresolved_name_in_assign_target(
    target: &mut AssignTarget,
    name: &str,
    binding_id: HirId,
) {
    match target {
        AssignTarget::Name(_) => {}
        AssignTarget::Field { receiver, .. } => {
            replace_unresolved_name(receiver, name, binding_id);
        }
        AssignTarget::Index { base, indices } => {
            replace_unresolved_name(base, name, binding_id);
            for idx in indices {
                replace_unresolved_name(idx, name, binding_id);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests_coverage;
