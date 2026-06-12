use super::*;

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

    crate::pre_typecheck::pre_typecheck(&mut hir, &lang_items, next_id);

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
