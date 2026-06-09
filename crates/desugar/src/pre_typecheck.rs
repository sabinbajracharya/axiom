//! Pre-typecheck desugaring: `catch`, `else`, `ListLit`.
//!
//! Requires `LangItems` for `List::new` / `List::with_capacity` / `List::push`.
//! Does NOT touch `?` (needs type information).

use crate::helpers::DesugarCtx;
use resolver::hir_types::*;
use resolver::lang::LangItems;
use resolver::HirDiagnostic;

/// Run pre-typecheck desugaring on a fully name-resolved HIR.
/// Sugar expressions (`catch`, `else`, `ListLit`) are rewritten in-place.
/// Returns the next fresh ID to allocate.
pub fn pre_typecheck(hir: &mut Hir, lang_items: &LangItems, next_id: usize) -> usize {
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
    // pass resolved (e.g. `catch |e|` capture bindings, iterator loop vars).
    hir.diagnostics.retain(|d| {
        !matches!(d, HirDiagnostic::UnresolvedName { name, .. } if ctx.fixed_names.contains(name))
    });
    ctx.next_id
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
        Expr::Loop(e) => {
            desugar_loop_kind(&mut e.kind, ctx);
            if matches!(e.kind, LoopKind::Iterator { .. }) {
                *expr = desugar_iterator_loop(e, ctx);
            }
        }
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
            // The post-typecheck desugar pass handles it.
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
        crate::helpers::replace_unresolved_name(&mut fallback, name, binding_id);
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
        crate::helpers::replace_unresolved_name(&mut fallback, name, err_binding_id);
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
        qualifier: Some(resolver::lang::LIST.to_string()),
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
        qualifier: Some(resolver::lang::LIST.to_string()),
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

/// Desugar `loop x in xs { body }` into:
/// ```ignore
/// val __iter_N = xs.into_iter()
/// loop {
///     match __iter_N.next() {
///         Some(x) => { body }
///         None    => break
///     }
/// }
/// ```
/// The `into_iter()` and `next()` calls are resolved by typeck against the
/// iterable's type, so no lang items are required for the desugar itself.
/// Build the `__iter_N.next()` call and a temp variable to store its result.
fn build_next_call(
    iter_temp_name: &str,
    iter_temp_id: HirId,
    ctx: &mut DesugarCtx,
) -> (String, HirId, Stmt) {
    let receiver_path = Expr::Path(PathExpr {
        id: ctx.fresh_id(),
        name_ref: NameRef::resolved(iter_temp_id, iter_temp_name),
    });
    let next_call = Expr::MethodCall(MethodCallExpr {
        id: ctx.fresh_id(),
        receiver: Box::new(receiver_path),
        method: "next".to_string(),
        args: Vec::new(),
    });
    let opt_temp_name = ctx.fresh_temp_name();
    let opt_temp_id = ctx.fresh_id();
    let opt_var_stmt = Stmt::VarStmt(VarStmt {
        id: opt_temp_id,
        pattern: Pattern::Ident(IdentPat {
            id: opt_temp_id,
            name: opt_temp_name.clone(),
            binding: Some(opt_temp_id),
            span: lexer::Span { lo: 0, hi: 0 },
        }),
        ty: None,
        value: next_call,
    });
    (opt_temp_name, opt_temp_id, opt_var_stmt)
}

/// Build a `match (opt_temp_name) { Some(x) => body, None => break }` expression.
#[allow(clippy::too_many_arguments)]
fn build_iteration_match(
    binding_name: &str,
    binding_id: HirId,
    opt_temp_name: &str,
    opt_temp_id: HirId,
    body_block: Block,
    ctx: &mut DesugarCtx,
) -> Expr {
    let some_pat = Pattern::TupleStruct(TupleStructPat {
        id: ctx.fresh_id(),
        path: NameRef::unresolved("Some"),
        fields: vec![Pattern::Ident(IdentPat {
            id: binding_id,
            name: binding_name.to_string(),
            binding: Some(binding_id),
            span: lexer::Span { lo: 0, hi: 0 },
        })],
    });
    let some_arm = MatchArm {
        pattern: some_pat,
        guard: None,
        body: Expr::Block(body_block),
    };
    let none_pat = Pattern::TupleStruct(TupleStructPat {
        id: ctx.fresh_id(),
        path: NameRef::unresolved("None"),
        fields: Vec::new(),
    });
    let none_body = Expr::Block(Block {
        id: ctx.fresh_id(),
        stmts: vec![Stmt::BreakStmt(BreakStmt {
            id: ctx.fresh_id(),
            value: None,
        })],
        tail: None,
    });
    let none_arm = MatchArm {
        pattern: none_pat,
        guard: None,
        body: none_body,
    };
    Expr::Match(MatchExpr {
        id: ctx.fresh_id(),
        scrutinee: Box::new(Expr::Path(PathExpr {
            id: ctx.fresh_id(),
            name_ref: NameRef::resolved(opt_temp_id, opt_temp_name),
        })),
        arms: vec![some_arm, none_arm],
    })
}

/// Take ownership of the LoopKind::Iterator's fields, replacing it with a dummy.
fn take_iterator_fields(loop_expr: &mut LoopExpr) -> (String, HirId, Expr, Block) {
    let dummy_body = Block {
        id: HirId(0),
        stmts: Vec::new(),
        tail: None,
    };
    let dummy_kind = LoopKind::Infinite(dummy_body);
    let mut kind = std::mem::replace(&mut loop_expr.kind, dummy_kind);
    let LoopKind::Iterator {
        binding,
        binding_id,
        iterable,
        body,
    } = &mut kind
    else {
        unreachable!("called on non-iterator loop")
    };
    let iterable_expr = *std::mem::replace(
        iterable,
        Box::new(Expr::Lit(LitExpr {
            id: HirId(0),
            kind: LitKind::Unit,
        })),
    );
    let body_block = std::mem::replace(
        body,
        Block {
            id: HirId(0),
            stmts: Vec::new(),
            tail: None,
        },
    );
    let binding_name = std::mem::take(binding);
    (binding_name, *binding_id, iterable_expr, body_block)
}

/// Build `val __iter_N = <expr>.into_iter()` — the iterator variable statement.
fn build_iter_var_stmt(iterable_expr: Expr, ctx: &mut DesugarCtx) -> (String, HirId, Stmt) {
    let into_iter_id = ctx.fresh_id();
    let into_iter_call = Expr::MethodCall(MethodCallExpr {
        id: into_iter_id,
        receiver: Box::new(iterable_expr),
        method: "into_iter".to_string(),
        args: Vec::new(),
    });
    let iter_temp_name = ctx.fresh_temp_name();
    let iter_temp_id = ctx.fresh_id();
    let stmt = Stmt::VarStmt(VarStmt {
        id: iter_temp_id,
        pattern: Pattern::Ident(IdentPat {
            id: iter_temp_id,
            name: iter_temp_name.clone(),
            binding: Some(iter_temp_id),
            span: lexer::Span { lo: 0, hi: 0 },
        }),
        ty: None,
        value: into_iter_call,
    });
    (iter_temp_name, iter_temp_id, stmt)
}

/// Main desugar for `loop x in xs { body }`.
/// Transforms into: `{ val __iter = xs.into_iter(); loop { val __opt = __iter.next(); match __opt { Some(x) => body, None => break } } }`
fn desugar_iterator_loop(loop_expr: &mut LoopExpr, ctx: &mut DesugarCtx) -> Expr {
    let (binding_name, binding_id, iterable_expr, mut body_block) = take_iterator_fields(loop_expr);
    crate::helpers::replace_unresolved_name_in_block(&mut body_block, &binding_name, binding_id);

    let (iter_temp_name, iter_temp_id, iter_var_stmt) = build_iter_var_stmt(iterable_expr, ctx);
    let (opt_temp_name, opt_temp_id, opt_var_stmt) =
        build_next_call(&iter_temp_name, iter_temp_id, ctx);
    let match_expr = build_iteration_match(
        &binding_name,
        binding_id,
        &opt_temp_name,
        opt_temp_id,
        body_block,
        ctx,
    );

    let loop_body = Block {
        id: ctx.fresh_id(),
        stmts: vec![
            opt_var_stmt,
            Stmt::ExprStmt(ExprStmt {
                id: ctx.fresh_id(),
                expr: match_expr,
            }),
        ],
        tail: None,
    };
    let infinite_loop = Expr::Loop(LoopExpr {
        id: ctx.fresh_id(),
        kind: LoopKind::Infinite(loop_body),
    });

    let result = Expr::Block(Block {
        id: ctx.fresh_id(),
        stmts: vec![iter_var_stmt],
        tail: Some(Box::new(infinite_loop)),
    });

    ctx.fixed_names.push(iter_temp_name);
    ctx.fixed_names.push(binding_name);
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests_coverage;
