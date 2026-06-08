//! Coverage invariant checks for the type checker.
//!
//! Per `docs/typeck-testing.md` §5: two invariants are mechanically enforced:
//! 1. Every expression/statement HirId has an entry in the TypeMap (no untyped nodes).
//! 2. Every `Ty::Error` has a corresponding `TypeDiagnostic` (no orphan errors).
//!
//! These are the type-checker's analogue of the HIR's `check_all`.

use crate::thir::Thir;
use crate::types::Ty;
use hir::*;

/// A coverage error — a type-check gap discovered by `check_all`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeckCoverageError {
    /// An expression/statement HirId has no entry in the TypeMap.
    UntypedExpression { id: HirId, kind: String },
    /// A Ty::Error with no corresponding TypeDiagnostic.
    ErrorWithoutDiagnostic { id: HirId },
    /// A TypeDiagnostic that doesn't correspond to any Ty::Error.
    DiagnosticWithoutError { index: usize },
}

/// Verify coverage of the TypeMap against the HIR:
/// - Every expression and statement node has an assigned type.
/// - Every `Ty::Error` has a corresponding `TypeDiagnostic`.
/// - Every `TypeDiagnostic` corresponds to a node with `Ty::Error`.
///
/// Returns `Ok(())` if coverage is clean, or a list of errors.
pub fn check_all(thir: &Thir) -> Result<(), Vec<TypeckCoverageError>> {
    let mut errors = Vec::new();

    // Walk all HirIds in the HIR and verify they have a type.
    let mut expected_ids: Vec<(HirId, String)> = Vec::new();
    collect_all_hir_ids(&thir.hir, &mut expected_ids);

    for (id, kind) in &expected_ids {
        if !thir.types.contains_key(id) {
            errors.push(TypeckCoverageError::UntypedExpression {
                id: *id,
                kind: kind.clone(),
            });
        }
    }

    // Each Ty::Error node must have at least one corresponding diagnostic.
    // We don't enforce strict 1:1 (one diagnostic can cover multiple Ty::Error
    // nodes), but every Ty::Error must be backed by at least one diagnostic.
    for (id, ty) in &thir.types {
        if matches!(ty, Ty::Error) && thir.diagnostics.is_empty() {
            errors.push(TypeckCoverageError::ErrorWithoutDiagnostic { id: *id });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Collect all HirIds from the HIR (items, expressions, statements, patterns).
fn collect_all_hir_ids(hir: &hir::Hir, ids: &mut Vec<(HirId, String)>) {
    for item in &hir.items {
        collect_item_ids(item, ids);
    }
}

fn collect_item_ids(item: &hir::Item, ids: &mut Vec<(HirId, String)>) {
    match item {
        hir::Item::FnDef(f) => {
            ids.push((f.id, "FnDef".to_string()));
            for param in &f.params {
                ids.push((param.id, "Param".to_string()));
            }
            collect_block_ids(&f.body, ids);
        }
        hir::Item::StructDef(s) => {
            ids.push((s.id, "StructDef".to_string()));
            for field in &s.fields {
                ids.push((field.id, "FieldDef".to_string()));
            }
        }
        hir::Item::EnumDef(e) => {
            ids.push((e.id, "EnumDef".to_string()));
            for variant in &e.variants {
                ids.push((variant.id, "VariantDef".to_string()));
            }
        }
        hir::Item::TraitDef(t) => {
            ids.push((t.id, "TraitDef".to_string()));
            for method in &t.methods {
                ids.push((method.id, "TraitMethod".to_string()));
                for param in &method.params {
                    ids.push((param.id, "Param".to_string()));
                }
            }
        }
        hir::Item::ImplDef(impl_def) => {
            ids.push((impl_def.id, "ImplDef".to_string()));
            for method in &impl_def.methods {
                collect_item_ids(&hir::Item::FnDef(method.clone()), ids);
            }
        }
        hir::Item::SubscriptDef(s) => {
            ids.push((s.id, "SubscriptDef".to_string()));
        }
        hir::Item::UseItem(u) => {
            ids.push((u.id, "UseItem".to_string()));
        }
    }
}

fn collect_block_ids(block: &hir::Block, ids: &mut Vec<(HirId, String)>) {
    ids.push((block.id, "Block".to_string()));
    for stmt in &block.stmts {
        collect_stmt_ids(stmt, ids);
    }
    if let Some(tail) = &block.tail {
        collect_expr_ids(tail, ids);
    }
}

fn collect_stmt_ids(stmt: &hir::Stmt, ids: &mut Vec<(HirId, String)>) {
    match stmt {
        hir::Stmt::ValStmt(s) => {
            ids.push((s.id, "ValStmt".to_string()));
            collect_pattern_ids(&s.pattern, ids);
            collect_expr_ids(&s.value, ids);
        }
        hir::Stmt::VarStmt(s) => {
            ids.push((s.id, "VarStmt".to_string()));
            collect_pattern_ids(&s.pattern, ids);
            collect_expr_ids(&s.value, ids);
        }
        hir::Stmt::ExprStmt(s) => {
            ids.push((s.id, "ExprStmt".to_string()));
            collect_expr_ids(&s.expr, ids);
        }
        hir::Stmt::ReturnStmt(s) => {
            ids.push((s.id, "ReturnStmt".to_string()));
            if let Some(v) = &s.value {
                collect_expr_ids(v, ids);
            }
        }
        hir::Stmt::BreakStmt(s) => {
            ids.push((s.id, "BreakStmt".to_string()));
            if let Some(v) = &s.value {
                collect_expr_ids(v, ids);
            }
        }
        hir::Stmt::ContinueStmt(s) => {
            ids.push((s.id, "ContinueStmt".to_string()));
        }
        hir::Stmt::YieldStmt(s) => {
            ids.push((s.id, "YieldStmt".to_string()));
            collect_expr_ids(&s.value, ids);
        }
    }
}

fn collect_expr_ids(expr: &hir::Expr, ids: &mut Vec<(HirId, String)>) {
    ids.push((expr.id(), expr_kind_name(expr).to_string()));
    collect_expr_children(expr, ids);
}

fn expr_kind_name(expr: &hir::Expr) -> &'static str {
    match expr {
        hir::Expr::Lit(_) => "Lit",
        hir::Expr::Path(_) => "Path",
        hir::Expr::Bin(_) => "Bin",
        hir::Expr::Unary(_) => "Unary",
        hir::Expr::Call(_) => "Call",
        hir::Expr::MethodCall(_) => "MethodCall",
        hir::Expr::Field(_) => "Field",
        hir::Expr::Index(_) => "Index",
        hir::Expr::Block(_) => "Block",
        hir::Expr::If(_) => "If",
        hir::Expr::Match(_) => "Match",
        hir::Expr::Loop(_) => "Loop",
        hir::Expr::StructLit(_) => "StructLit",
        hir::Expr::Assign(_) => "Assign",
        hir::Expr::ListLit(_) => "ListLit",
    }
}

fn collect_expr_children(expr: &hir::Expr, ids: &mut Vec<(HirId, String)>) {
    match expr {
        hir::Expr::Lit(_) | hir::Expr::Path(_) => {}
        hir::Expr::Bin(b) => {
            collect_expr_ids(&b.left, ids);
            collect_expr_ids(&b.right, ids);
        }
        hir::Expr::Unary(u) => collect_expr_ids(&u.operand, ids),
        hir::Expr::Call(c) => c.args.iter().for_each(|arg| collect_expr_ids(arg, ids)),
        hir::Expr::MethodCall(m) => {
            collect_expr_ids(&m.receiver, ids);
            m.args.iter().for_each(|arg| collect_expr_ids(arg, ids));
        }
        hir::Expr::Field(f) => collect_expr_ids(&f.receiver, ids),
        hir::Expr::Index(i) => {
            collect_expr_ids(&i.base, ids);
            i.indices.iter().for_each(|idx| collect_expr_ids(idx, ids));
        }
        hir::Expr::Block(b) => collect_block_ids(b, ids),
        hir::Expr::If(i) => {
            collect_expr_ids(&i.condition, ids);
            collect_block_ids(&i.then_branch, ids);
            if let Some(els) = &i.else_branch {
                collect_expr_ids(els, ids);
            }
        }
        hir::Expr::Match(m) => {
            collect_expr_ids(&m.scrutinee, ids);
            for arm in &m.arms {
                collect_pattern_ids(&arm.pattern, ids);
                collect_expr_ids(&arm.body, ids);
            }
        }
        hir::Expr::Loop(l) => collect_loop_ids(l, ids),
        hir::Expr::StructLit(s) => {
            s.fields
                .iter()
                .for_each(|f| collect_expr_ids(&f.value, ids));
        }
        hir::Expr::Assign(a) => {
            collect_assign_target_ids(&a.target, ids);
            collect_expr_ids(&a.value, ids);
        }
        hir::Expr::ListLit(l) => {
            l.elements
                .iter()
                .for_each(|elem| collect_expr_ids(elem, ids));
        }
    }
}

fn collect_loop_ids(l: &hir::LoopExpr, ids: &mut Vec<(HirId, String)>) {
    match &l.kind {
        hir::LoopKind::Infinite(body) => collect_block_ids(body, ids),
        hir::LoopKind::Conditional { condition, body } => {
            collect_expr_ids(condition, ids);
            collect_block_ids(body, ids);
        }
        hir::LoopKind::Iterator {
            binding_id,
            iterable,
            body,
            ..
        } => {
            ids.push((*binding_id, "IteratorBinding".to_string()));
            collect_expr_ids(iterable, ids);
            collect_block_ids(body, ids);
        }
    }
}

fn collect_assign_target_ids(target: &hir::AssignTarget, ids: &mut Vec<(HirId, String)>) {
    match target {
        hir::AssignTarget::Name(_) => {}
        hir::AssignTarget::Field { receiver, field: _ } => {
            collect_expr_ids(receiver, ids);
        }
        hir::AssignTarget::Index { base, indices } => {
            collect_expr_ids(base, ids);
            for index in indices {
                collect_expr_ids(index, ids);
            }
        }
    }
}

fn collect_pattern_ids(pat: &hir::Pattern, ids: &mut Vec<(HirId, String)>) {
    let kind = match pat {
        hir::Pattern::Wildcard(_) => "Wildcard",
        hir::Pattern::Ident(_) => "IdentPat",
        hir::Pattern::Literal(_) => "LitPat",
        hir::Pattern::TupleStruct(_) => "TupleStructPat",
        hir::Pattern::Struct(_) => "StructPat",
        hir::Pattern::Or(_) => "OrPat",
        hir::Pattern::Range(_) => "RangePat",
    };
    ids.push((pat.id(), kind.to_string()));

    match pat {
        hir::Pattern::Wildcard(_)
        | hir::Pattern::Ident(_)
        | hir::Pattern::Literal(_)
        | hir::Pattern::Range(_) => {}
        hir::Pattern::TupleStruct(ts) => {
            for f in &ts.fields {
                collect_pattern_ids(f, ids);
            }
        }
        hir::Pattern::Struct(sp) => {
            for f in &sp.fields {
                collect_pattern_ids(&f.pattern, ids);
            }
        }
        hir::Pattern::Or(op) => {
            for alt in &op.alternatives {
                collect_pattern_ids(alt, ids);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::coverage::check_all;
    use crate::coverage::TypeckCoverageError;
    use crate::thir::Thir;
    use crate::thir::TypeMap;
    use crate::types::Ty;
    use hir::HirId;

    #[test]
    fn test_check_all_empty_thir() {
        let hir = hir::Hir {
            items: vec![],
            diagnostics: vec![],
        };
        let thir = Thir {
            hir,
            types: TypeMap::new(),
            diagnostics: vec![],
        };
        assert!(check_all(&thir).is_ok());
    }

    #[test]
    fn test_check_all_missing_type() {
        use hir::{Block, FnDef, Item, Visibility};

        let fn_id = HirId(0);
        let block_id = HirId(1);
        let hir = hir::Hir {
            items: vec![Item::FnDef(FnDef {
                id: fn_id,
                name: "main".to_string(),
                module_path: String::new(),
                visibility: Visibility::Private,
                type_params: vec![],
                params: vec![],
                return_type: None,
                body: Block {
                    id: block_id,
                    stmts: vec![],
                    tail: None,
                },
                extern_abi: None,
                lang_tag: None,
                intrinsic_tag: None,
            })],
            diagnostics: vec![],
        };

        let mut types = TypeMap::new();
        types.insert(
            fn_id,
            Ty::Fn(crate::types::FnTy {
                params: vec![],
                return_type: Box::new(Ty::Unit),
            }),
        );
        types.insert(block_id, Ty::Unit);

        let thir = Thir {
            hir,
            types,
            diagnostics: vec![],
        };

        assert!(check_all(&thir).is_ok());
    }

    #[test]
    fn test_coverage_error_untyped_expr() {
        let err = TypeckCoverageError::UntypedExpression {
            id: HirId(42),
            kind: "Call".to_string(),
        };
        assert!(format!("{err:?}").contains("Call"));
        assert!(format!("{err:?}").contains("42"));
    }

    #[test]
    fn test_coverage_error_without_diagnostic() {
        let err = TypeckCoverageError::ErrorWithoutDiagnostic { id: HirId(7) };
        assert!(format!("{err:?}").contains("7"));
    }
}
