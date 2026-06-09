//! Tests for post-typecheck desugaring: `?` → `match`.

use resolver::hir_types::*;
use typecheck::types::{ErrorSetTy, InstanceTy};
use typecheck::{Ty, TypeMap};

fn make_question_expr(id: usize, inner: Expr) -> Expr {
    Expr::Question(QuestionExpr {
        id: HirId(id),
        expr: Box::new(inner),
    })
}

fn make_path(id: usize, name: &str) -> Expr {
    Expr::Path(PathExpr {
        id: HirId(id),
        name_ref: NameRef::unresolved(name),
    })
}

fn make_fn_def(name: &str, body: Block) -> Item {
    Item::FnDef(FnDef {
        id: HirId(0),
        name: name.to_string(),
        module_path: String::new(),
        visibility: Visibility::Public,
        type_params: vec![],
        params: vec![],
        return_type: None,
        body,
        extern_abi: None,
        lang_tag: None,
        intrinsic_tag: None,
    })
}

fn make_block(id: usize, stmts: Vec<Stmt>, tail: Option<Expr>) -> Block {
    Block {
        id: HirId(id),
        stmts,
        tail: tail.map(Box::new),
    }
}

fn option_ty() -> Ty {
    Ty::Instance(InstanceTy {
        name: "Option".to_string(),
        def_id: HirId(100),
        args: vec![Ty::Int],
    })
}

fn result_ty() -> Ty {
    Ty::Instance(InstanceTy {
        name: "Result".to_string(),
        def_id: HirId(100),
        args: vec![Ty::Int, Ty::Int],
    })
}

// ── Option desugaring ────────────────────────────────────────────────────────

#[test]
fn test_question_option_desugars_to_match() {
    let mut hir = Hir {
        items: vec![make_fn_def(
            "f",
            make_block(10, vec![], Some(make_question_expr(1, make_path(2, "x")))),
        )],
        diagnostics: vec![],
    };

    let mut types = TypeMap::new();
    types.insert(HirId(2), option_ty());

    let next_id = super::post_typecheck(&mut hir, &types, 200);

    assert!(next_id > 200);

    let item = &hir.items[0];
    match item {
        Item::FnDef(fd) => match fd.body.tail.as_deref() {
            Some(Expr::Match(m)) => {
                assert_eq!(m.arms.len(), 2);
                // First arm: Some(v) => v
                match &m.arms[0].pattern {
                    Pattern::TupleStruct(p) => {
                        assert_eq!(p.path, NameRef::unresolved("Some"));
                        assert_eq!(p.fields.len(), 1);
                    }
                    other => panic!("expected TupleStruct pattern for Some, got {:?}", other),
                }
                // Second arm: _ => return None (Option failure)
                match &m.arms[1].pattern {
                    Pattern::Wildcard(_) => {}
                    other => panic!("expected Wildcard pattern for None arm, got {:?}", other),
                }
                // Verify the failure arm contains `return None`
                match &m.arms[1].body {
                    Expr::Block(b) => match &b.stmts[0] {
                        Stmt::ReturnStmt(r) => match &r.value {
                            Some(Expr::Path(p)) => {
                                assert_eq!(p.name_ref, NameRef::unresolved("None"));
                            }
                            other => panic!("expected return None, got {:?}", other),
                        },
                        other => panic!("expected ReturnStmt, got {:?}", other),
                    },
                    other => panic!("expected Block in failure arm, got {:?}", other),
                }
            }
            other => panic!("expected Match expr, got {:?}", other),
        },
        other => panic!("expected FnDef, got {:?}", other),
    }
}

// ── Result desugaring ────────────────────────────────────────────────────────

#[test]
fn test_question_result_desugars_to_match() {
    let mut hir = Hir {
        items: vec![make_fn_def(
            "f",
            make_block(10, vec![], Some(make_question_expr(1, make_path(2, "x")))),
        )],
        diagnostics: vec![],
    };

    let mut types = TypeMap::new();
    types.insert(HirId(2), result_ty());

    let next_id = super::post_typecheck(&mut hir, &types, 200);

    assert!(next_id > 200);

    let item = &hir.items[0];
    match item {
        Item::FnDef(fd) => match fd.body.tail.as_deref() {
            Some(Expr::Match(m)) => {
                assert_eq!(m.arms.len(), 2);
                // First arm: Ok(v) => v
                match &m.arms[0].pattern {
                    Pattern::TupleStruct(p) => {
                        assert_eq!(p.path, NameRef::unresolved("Ok"));
                        assert_eq!(p.fields.len(), 1);
                    }
                    other => panic!("expected TupleStruct pattern for Ok, got {:?}", other),
                }
                // Second arm: Err(e) => return Err(e)
                match &m.arms[1].pattern {
                    Pattern::TupleStruct(p) => {
                        assert_eq!(p.path, NameRef::unresolved("Err"));
                        assert_eq!(p.fields.len(), 1);
                    }
                    other => panic!("expected TupleStruct pattern for Err, got {:?}", other),
                }
                // Verify the failure arm contains `return Err(e)`
                match &m.arms[1].body {
                    Expr::Block(b) => match &b.stmts[0] {
                        Stmt::ReturnStmt(r) => match &r.value {
                            Some(Expr::Call(c)) => {
                                assert_eq!(c.callee, NameRef::unresolved("Err"));
                                assert_eq!(c.args.len(), 1);
                            }
                            other => panic!("expected return Err(e), got {:?}", other),
                        },
                        other => panic!("expected ReturnStmt, got {:?}", other),
                    },
                    other => panic!("expected Block in failure arm, got {:?}", other),
                }
            }
            other => panic!("expected Match expr, got {:?}", other),
        },
        other => panic!("expected FnDef, got {:?}", other),
    }
}

// ── Idempotency ──────────────────────────────────────────────────────────────

#[test]
fn test_post_typecheck_idempotent() {
    let mut hir = Hir {
        items: vec![make_fn_def(
            "f",
            make_block(10, vec![], Some(make_question_expr(1, make_path(2, "x")))),
        )],
        diagnostics: vec![],
    };

    let mut types = TypeMap::new();
    types.insert(HirId(2), option_ty());

    let next_id = super::post_typecheck(&mut hir, &types, 200);

    let hir_after_first = hir.clone();

    super::post_typecheck(&mut hir, &types, next_id);

    let hir_after_second = hir;
    assert_eq!(
        format!("{:?}", hir_after_first),
        format!("{:?}", hir_after_second)
    );
}

// ── No-Question invariant ────────────────────────────────────────────────────

fn contains_question(hir: &Hir) -> bool {
    hir.items.iter().any(contains_question_item)
}

fn contains_question_item(item: &Item) -> bool {
    match item {
        Item::FnDef(fd) => contains_question_block(&fd.body),
        Item::ImplDef(i) => {
            i.methods.iter().any(|m| contains_question_block(&m.body))
                || i.subscripts
                    .iter()
                    .any(|s| contains_question_block(&s.body))
        }
        Item::TraitDef(t) => t
            .methods
            .iter()
            .filter_map(|m| m.body.as_ref())
            .any(contains_question_block),
        Item::SubscriptDef(s) => contains_question_block(&s.body),
        Item::StructDef(_) | Item::EnumDef(_) | Item::UseItem(_) | Item::ErrorSetDef(_) => false,
    }
}

fn contains_question_block(block: &Block) -> bool {
    block.stmts.iter().any(contains_question_stmt)
        || block
            .tail
            .as_ref()
            .is_some_and(|b| contains_question_expr(b))
}

fn contains_question_stmt(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::ValStmt(s) => contains_question_expr(&s.value),
        Stmt::VarStmt(s) => contains_question_expr(&s.value),
        Stmt::ExprStmt(s) => contains_question_expr(&s.expr),
        Stmt::ReturnStmt(s) => s.value.as_ref().is_some_and(contains_question_expr),
        Stmt::BreakStmt(s) => s.value.as_ref().is_some_and(contains_question_expr),
        Stmt::ContinueStmt(_) | Stmt::YieldStmt(_) => false,
    }
}

fn contains_question_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Question(_) => true,
        Expr::Bin(e) => contains_question_expr(&e.left) || contains_question_expr(&e.right),
        Expr::Unary(e) => contains_question_expr(&e.operand),
        Expr::Call(e) => e.args.iter().any(contains_question_expr),
        Expr::MethodCall(e) => {
            contains_question_expr(&e.receiver) || e.args.iter().any(contains_question_expr)
        }
        Expr::Field(e) => contains_question_expr(&e.receiver),
        Expr::Index(e) => {
            contains_question_expr(&e.base) || e.indices.iter().any(contains_question_expr)
        }
        Expr::Block(e) => contains_question_block(e),
        Expr::If(e) => {
            contains_question_expr(&e.condition)
                || contains_question_block(&e.then_branch)
                || e.else_branch
                    .as_ref()
                    .is_some_and(|b| contains_question_expr(b))
        }
        Expr::Match(e) => {
            contains_question_expr(&e.scrutinee)
                || e.arms.iter().any(|a| {
                    a.guard.as_ref().is_some_and(contains_question_expr)
                        || contains_question_expr(&a.body)
                })
        }
        Expr::Loop(e) => match &e.kind {
            LoopKind::Infinite(b) => contains_question_block(b),
            LoopKind::Conditional { condition, body } => {
                contains_question_expr(condition) || contains_question_block(body)
            }
            LoopKind::Iterator { iterable, body, .. } => {
                contains_question_expr(iterable) || contains_question_block(body)
            }
        },
        Expr::StructLit(e) => e.fields.iter().any(|f| contains_question_expr(&f.value)),
        Expr::ListLit(e) => e.elements.iter().any(contains_question_expr),
        Expr::Assign(e) => {
            contains_question_assign_target(&e.target) || contains_question_expr(&e.value)
        }
        Expr::Lit(_) | Expr::Path(_) | Expr::Catch(_) | Expr::Else(_) => false,
    }
}

fn contains_question_assign_target(target: &AssignTarget) -> bool {
    match target {
        AssignTarget::Name(_) => false,
        AssignTarget::Field { receiver, .. } => contains_question_expr(receiver),
        AssignTarget::Index { base, indices } => {
            contains_question_expr(base) || indices.iter().any(contains_question_expr)
        }
    }
}

#[test]
fn test_no_question_after_post_typecheck() {
    let mut hir = Hir {
        items: vec![make_fn_def(
            "f",
            make_block(10, vec![], Some(make_question_expr(1, make_path(2, "x")))),
        )],
        diagnostics: vec![],
    };

    let mut types = TypeMap::new();
    types.insert(HirId(2), option_ty());

    super::post_typecheck(&mut hir, &types, 200);

    assert!(
        !contains_question(&hir),
        "Expr::Question should not survive post_typecheck"
    );
}

// ── Nested ? expressions ─────────────────────────────────────────────────────

#[test]
fn test_nested_question_desugars_all() {
    let inner_question = make_question_expr(1, make_path(2, "x"));
    let outer_question = make_question_expr(3, inner_question);

    let mut hir = Hir {
        items: vec![make_fn_def(
            "f",
            make_block(10, vec![], Some(outer_question)),
        )],
        diagnostics: vec![],
    };

    let mut types = TypeMap::new();
    types.insert(HirId(2), option_ty());

    super::post_typecheck(&mut hir, &types, 200);

    assert!(
        !contains_question(&hir),
        "Nested Expr::Question should all be desugared"
    );
}

// ── ? in statement position ──────────────────────────────────────────────────

#[test]
fn test_question_in_val_stmt() {
    let question = make_question_expr(1, make_path(2, "x"));
    let val_stmt = Stmt::ValStmt(ValStmt {
        id: HirId(5),
        pattern: Pattern::Ident(IdentPat {
            id: HirId(6),
            name: "y".to_string(),
            binding: None,
            span: lexer::Span { lo: 0, hi: 0 },
        }),
        ty: None,
        value: question,
    });

    let mut hir = Hir {
        items: vec![make_fn_def("f", make_block(10, vec![val_stmt], None))],
        diagnostics: vec![],
    };

    let mut types = TypeMap::new();
    types.insert(HirId(2), option_ty());

    super::post_typecheck(&mut hir, &types, 200);

    assert!(
        !contains_question(&hir),
        "Expr::Question in val stmt should be desugared"
    );
}

// ── ErrorSet type → Result desugaring ────────────────────────────────────────

#[test]
fn test_question_errorset_desugars_as_result() {
    let mut hir = Hir {
        items: vec![make_fn_def(
            "f",
            make_block(10, vec![], Some(make_question_expr(1, make_path(2, "x")))),
        )],
        diagnostics: vec![],
    };

    let mut types = TypeMap::new();
    types.insert(
        HirId(2),
        Ty::ErrorSet(ErrorSetTy {
            name: "IO".to_string(),
            def_id: HirId(100),
            variant_names: vec!["NotFound".to_string()],
        }),
    );

    super::post_typecheck(&mut hir, &types, 200);

    // ErrorSet → Result desugaring (Err arm, not wildcard)
    let item = &hir.items[0];
    match item {
        Item::FnDef(fd) => match fd.body.tail.as_deref() {
            Some(Expr::Match(m)) => match &m.arms[1].pattern {
                Pattern::TupleStruct(p) => {
                    assert_eq!(p.path, NameRef::unresolved("Err"));
                }
                other => panic!("expected Err pattern for ErrorSet, got {:?}", other),
            },
            other => panic!("expected Match, got {:?}", other),
        },
        other => panic!("expected FnDef, got {:?}", other),
    }
}
