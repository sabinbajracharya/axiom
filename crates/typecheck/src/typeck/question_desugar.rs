//! Post-typecheck desugar: rewrites `?` expressions into match expressions
//! using type information from the typechecker.
//!
//! Pre-typecheck desugar handles `catch`, `else`, and `ListLit` (these don't
//! need type info). `?` is deferred because it can propagate either `Option`
//! (Some/None) or `Result` (Ok/Err), and the choice depends on the operand type.

use crate::thir::TypeMap;
use crate::types::Ty;
use resolver::*;

/// Desugar all remaining `QuestionExpr` nodes in the HIR.
/// Must run after typecheck so `types` contains the inferred type for each
/// `?` operand. Returns the next fresh ID to allocate (for generated match nodes).
pub fn desugar_question(hir: &mut Hir, types: &TypeMap, start_id: usize) -> usize {
    let mut ctx = QuestionDesugarCtx {
        next_id: start_id,
        temp_counter: 0,
    };
    for item in &mut hir.items {
        desugar_item(item, types, &mut ctx);
    }
    ctx.next_id
}

struct QuestionDesugarCtx {
    next_id: usize,
    temp_counter: usize,
}

impl QuestionDesugarCtx {
    fn fresh_id(&mut self) -> HirId {
        let id = HirId(self.next_id);
        self.next_id += 1;
        id
    }
}

fn desugar_item(item: &mut Item, types: &TypeMap, ctx: &mut QuestionDesugarCtx) {
    match item {
        Item::FnDef(f) => desugar_block(&mut f.body, types, ctx),
        Item::ImplDef(i) => {
            for method in &mut i.methods {
                desugar_block(&mut method.body, types, ctx);
            }
            for sub in &mut i.subscripts {
                desugar_block(&mut sub.body, types, ctx);
            }
        }
        Item::TraitDef(t) => {
            for method in &mut t.methods {
                if let Some(ref mut body) = method.body {
                    desugar_block(body, types, ctx);
                }
            }
        }
        Item::SubscriptDef(s) => desugar_block(&mut s.body, types, ctx),
        Item::StructDef(_) | Item::EnumDef(_) | Item::UseItem(_) | Item::ErrorSetDef(_) => {}
    }
}

fn desugar_block(block: &mut Block, types: &TypeMap, ctx: &mut QuestionDesugarCtx) {
    for stmt in &mut block.stmts {
        desugar_stmt(stmt, types, ctx);
    }
    if let Some(ref mut tail) = block.tail {
        desugar_expr(tail, types, ctx);
    }
}

fn desugar_stmt(stmt: &mut Stmt, types: &TypeMap, ctx: &mut QuestionDesugarCtx) {
    match stmt {
        Stmt::ValStmt(s) => desugar_expr(&mut s.value, types, ctx),
        Stmt::VarStmt(s) => desugar_expr(&mut s.value, types, ctx),
        Stmt::ExprStmt(s) => desugar_expr(&mut s.expr, types, ctx),
        Stmt::ReturnStmt(s) => {
            if let Some(ref mut v) = s.value {
                desugar_expr(v, types, ctx);
            }
        }
        Stmt::BreakStmt(s) => {
            if let Some(ref mut v) = s.value {
                desugar_expr(v, types, ctx);
            }
        }
        Stmt::ContinueStmt(_) | Stmt::YieldStmt(_) => {}
    }
}

fn desugar_expr(expr: &mut Expr, types: &TypeMap, ctx: &mut QuestionDesugarCtx) {
    match expr {
        Expr::Lit(_) | Expr::Path(_) => {}
        Expr::Bin(e) => {
            desugar_expr(&mut e.left, types, ctx);
            desugar_expr(&mut e.right, types, ctx);
        }
        Expr::Unary(e) => desugar_expr(&mut e.operand, types, ctx),
        Expr::Call(e) => {
            for arg in &mut e.args {
                desugar_expr(arg, types, ctx);
            }
        }
        Expr::MethodCall(e) => {
            desugar_expr(&mut e.receiver, types, ctx);
            for arg in &mut e.args {
                desugar_expr(arg, types, ctx);
            }
        }
        Expr::Field(e) => desugar_expr(&mut e.receiver, types, ctx),
        Expr::Index(e) => {
            desugar_expr(&mut e.base, types, ctx);
            for idx in &mut e.indices {
                desugar_expr(idx, types, ctx);
            }
        }
        Expr::Block(e) => desugar_block(e, types, ctx),
        Expr::If(e) => {
            desugar_expr(&mut e.condition, types, ctx);
            desugar_block(&mut e.then_branch, types, ctx);
            if let Some(ref mut else_branch) = e.else_branch {
                desugar_expr(else_branch, types, ctx);
            }
        }
        Expr::Match(e) => {
            desugar_expr(&mut e.scrutinee, types, ctx);
            for arm in &mut e.arms {
                if let Some(ref mut guard) = arm.guard {
                    desugar_expr(guard, types, ctx);
                }
                desugar_expr(&mut arm.body, types, ctx);
            }
        }
        Expr::Loop(e) => desugar_loop_kind(&mut e.kind, types, ctx),
        Expr::StructLit(e) => {
            for field in &mut e.fields {
                desugar_expr(&mut field.value, types, ctx);
            }
        }
        Expr::ListLit(_) => {
            // ListLit should already be desugared. If it survived (no-stdlib
            // path), leave it — the typechecker emitted a diagnostic.
        }
        Expr::Assign(e) => {
            desugar_assign_target(&mut e.target, types, ctx);
            desugar_expr(&mut e.value, types, ctx);
        }
        Expr::Question(q) => {
            desugar_expr(&mut q.expr, types, ctx);
            *expr = desugar_question_expr(q, types, ctx);
        }
        // catch, else, and ListLit are already desugared pre-typecheck.
        Expr::Catch(_) | Expr::Else(_) => {}
    }
}

fn desugar_loop_kind(kind: &mut LoopKind, types: &TypeMap, ctx: &mut QuestionDesugarCtx) {
    match kind {
        LoopKind::Infinite(b) => desugar_block(b, types, ctx),
        LoopKind::Conditional { condition, body } => {
            desugar_expr(condition, types, ctx);
            desugar_block(body, types, ctx);
        }
        LoopKind::Iterator { iterable, body, .. } => {
            desugar_expr(iterable, types, ctx);
            desugar_block(body, types, ctx);
        }
    }
}

fn desugar_assign_target(target: &mut AssignTarget, types: &TypeMap, ctx: &mut QuestionDesugarCtx) {
    match target {
        AssignTarget::Name(_) => {}
        AssignTarget::Field { receiver, .. } => desugar_expr(receiver, types, ctx),
        AssignTarget::Index { base, indices } => {
            desugar_expr(base, types, ctx);
            for idx in indices {
                desugar_expr(idx, types, ctx);
            }
        }
    }
}

/// Determine whether a `?` expression's operand is an Option type based on
/// the type map. If the type is `Option<T>`, desugar to `Some(v) => v, None => return None`.
/// If the type is `Result<T,E>` (or `E!T`), desugar to `Ok(v) => v, Err(e) => return Err(e)`.
/// If the type is unknown or error, default to Option desugaring (best effort).
fn desugar_question_expr(q: &QuestionExpr, types: &TypeMap, ctx: &mut QuestionDesugarCtx) -> Expr {
    let scrutinee = q.expr.clone();
    let is_option = match types.get(&q.expr.id()) {
        Some(Ty::Instance(inst)) => inst.name == "Option",
        Some(Ty::ErrorSet(_)) => false,
        _ => true,
    };

    let match_id = ctx.fresh_id();
    let success_arm = build_success_arm(is_option, ctx);
    let failure_arm = build_failure_arm(is_option, ctx);

    Expr::Match(MatchExpr {
        id: match_id,
        scrutinee,
        arms: vec![success_arm, failure_arm],
    })
}

fn build_success_arm(is_option: bool, ctx: &mut QuestionDesugarCtx) -> MatchArm {
    let success_variant = if is_option { "Some" } else { "Ok" };
    let binding_name = if is_option {
        format!("__q_some_{}", ctx.temp_counter)
    } else {
        format!("__q_ok_{}", ctx.temp_counter)
    };
    ctx.temp_counter += 1;
    let binding_id = ctx.fresh_id();
    let body_id = ctx.fresh_id();

    let success_pat = Pattern::TupleStruct(TupleStructPat {
        id: ctx.fresh_id(),
        path: NameRef::unresolved(success_variant),
        fields: vec![Pattern::Ident(IdentPat {
            id: binding_id,
            name: binding_name.clone(),
            binding: Some(binding_id),
            span: lexer::Span { lo: 0, hi: 0 },
        })],
    });

    MatchArm {
        pattern: success_pat,
        guard: None,
        body: Expr::Path(PathExpr {
            id: body_id,
            name_ref: NameRef::resolved(binding_id, &binding_name),
        }),
    }
}

fn build_failure_arm(is_option: bool, ctx: &mut QuestionDesugarCtx) -> MatchArm {
    if is_option {
        MatchArm {
            pattern: Pattern::Wildcard(ctx.fresh_id()),
            guard: None,
            body: Expr::Block(Block {
                id: ctx.fresh_id(),
                stmts: vec![Stmt::ReturnStmt(ReturnStmt {
                    id: ctx.fresh_id(),
                    value: Some(Expr::Path(PathExpr {
                        id: ctx.fresh_id(),
                        name_ref: NameRef::unresolved("None"),
                    })),
                })],
                tail: None,
            }),
        }
    } else {
        let failure_variant = "Err";
        let err_binding_name = format!("__q_err_{}", ctx.temp_counter);
        ctx.temp_counter += 1;
        let err_binding_id = ctx.fresh_id();
        let err_pat = Pattern::TupleStruct(TupleStructPat {
            id: ctx.fresh_id(),
            path: NameRef::unresolved(failure_variant),
            fields: vec![Pattern::Ident(IdentPat {
                id: err_binding_id,
                name: err_binding_name.clone(),
                binding: Some(err_binding_id),
                span: lexer::Span { lo: 0, hi: 0 },
            })],
        });
        MatchArm {
            pattern: err_pat,
            guard: None,
            body: Expr::Block(Block {
                id: ctx.fresh_id(),
                stmts: vec![Stmt::ReturnStmt(ReturnStmt {
                    id: ctx.fresh_id(),
                    value: Some(Expr::Call(CallExpr {
                        id: ctx.fresh_id(),
                        callee: NameRef::unresolved(failure_variant),
                        qualifier: None,
                        args: vec![Expr::Path(PathExpr {
                            id: ctx.fresh_id(),
                            name_ref: NameRef::resolved(err_binding_id, &err_binding_name),
                        })],
                    })),
                })],
                tail: None,
            }),
        }
    }
}
