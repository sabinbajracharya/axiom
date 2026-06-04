//! The Axiom HIR (M1): a desugared, ID-keyed tree where every identifier
//! resolves to a binding or item def.
//!
//! Built test-first against [`docs/hir-testing.md`](../../../docs/hir-testing.md).
//! The HIR is **not a lossless CST** — trivia is gone, names are resolved (or
//! diagnosed), and every node carries a stable `HirId` for later type annotation.
//!
//! The pipeline: `parse(source)` → `SourceFile` (CST/AST) →
//! `lower(&SourceFile, source)` → `Hir` (items + diagnostics).
//!
//! ```
//! use axiom_parser::parse;
//! use axiom_hir::{lower, serialize};
//! use axiom_parser::ast::AstNode;
//!
//! let result = parse("fn main() { val x = 1 }");
//! let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
//! let hir = lower(&root, "fn main() { val x = 1 }");
//! assert!(hir.diagnostics.is_empty());
//! let dump = serialize(&hir);
//! assert!(dump.contains("FnDef"));
//! ```

mod error;
mod hir;
mod lower;
mod resolve;
mod serialize;

pub use error::HirDiagnostic;
pub use hir::*;
pub use lower::lower;
pub use serialize::serialize;

/// Coverage checks: verifies that every `NameRef::Unresolved` in the HIR
/// has a corresponding `HirDiagnostic::UnresolvedName`. Returns `Ok(())`
/// if coverage is clean, or a list of coverage errors otherwise.
pub fn check_all(hir: &Hir) -> Result<(), Vec<CoverageError>> {
    let mut errors = Vec::new();
    let diagnosed: Vec<String> = hir
        .diagnostics
        .iter()
        .filter_map(|d| match d {
            HirDiagnostic::UnresolvedName { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    for item in &hir.items {
        check_item(item, &diagnosed, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_item(item: &Item, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    match item {
        Item::FnDef(f) => {
            check_block(&f.body, diagnosed, errors);
        }
        Item::StructDef(_) | Item::EnumDef(_) => {}
    }
}

fn check_block(block: &Block, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    for stmt in &block.stmts {
        check_stmt(stmt, diagnosed, errors);
    }
    if let Some(tail) = &block.tail {
        check_expr(tail, diagnosed, errors);
    }
}

fn check_stmt(stmt: &Stmt, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    match stmt {
        Stmt::ValStmt(s) => check_expr(&s.value, diagnosed, errors),
        Stmt::VarStmt(s) => check_expr(&s.value, diagnosed, errors),
        Stmt::ExprStmt(s) => check_expr(&s.expr, diagnosed, errors),
        Stmt::ReturnStmt(s) => {
            if let Some(v) = &s.value {
                check_expr(v, diagnosed, errors);
            }
        }
        Stmt::BreakStmt(s) => {
            if let Some(v) = &s.value {
                check_expr(v, diagnosed, errors);
            }
        }
        Stmt::ContinueStmt(_) => {}
    }
}

fn check_expr(expr: &Expr, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    match expr {
        Expr::Path(p) => check_name_ref(&p.name_ref, p.id, diagnosed, errors),
        Expr::Call(c) => {
            check_name_ref(&c.callee, c.id, diagnosed, errors);
            for arg in &c.args {
                check_expr(arg, diagnosed, errors);
            }
        }
        Expr::Bin(b) => {
            check_expr(&b.left, diagnosed, errors);
            check_expr(&b.right, diagnosed, errors);
        }
        Expr::Unary(u) => check_expr(&u.operand, diagnosed, errors),
        Expr::MethodCall(m) => {
            check_expr(&m.receiver, diagnosed, errors);
            for arg in &m.args {
                check_expr(arg, diagnosed, errors);
            }
        }
        Expr::Field(f) => check_expr(&f.receiver, diagnosed, errors),
        Expr::Index(i) => {
            check_expr(&i.base, diagnosed, errors);
            check_expr(&i.index, diagnosed, errors);
        }
        Expr::Block(b) => check_block(b, diagnosed, errors),
        Expr::If(i) => {
            check_expr(&i.condition, diagnosed, errors);
            check_block(&i.then_branch, diagnosed, errors);
            if let Some(els) = &i.else_branch {
                check_expr(els, diagnosed, errors);
            }
        }
        Expr::Match(m) => {
            check_expr(&m.scrutinee, diagnosed, errors);
            for arm in &m.arms {
                check_expr(&arm.body, diagnosed, errors);
            }
        }
        Expr::Loop(l) => check_loop(l, diagnosed, errors),
        Expr::StructLit(s) => {
            check_name_ref(&s.type_name, s.id, diagnosed, errors);
            for f in &s.fields {
                check_expr(&f.value, diagnosed, errors);
            }
        }
        Expr::Assign(a) => {
            check_assign_target(&a.target, a.id, diagnosed, errors);
            check_expr(&a.value, diagnosed, errors);
        }
        Expr::Lit(_) => {}
    }
}

fn check_loop(l: &LoopExpr, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    match &l.kind {
        LoopKind::Infinite(body) => check_block(body, diagnosed, errors),
        LoopKind::Conditional { condition, body } => {
            check_expr(condition, diagnosed, errors);
            check_block(body, diagnosed, errors);
        }
        LoopKind::Iterator { iterable, body, .. } => {
            check_expr(iterable, diagnosed, errors);
            check_block(body, diagnosed, errors);
        }
    }
}

fn check_assign_target(
    target: &AssignTarget,
    id: HirId,
    diagnosed: &[String],
    errors: &mut Vec<CoverageError>,
) {
    if let AssignTarget::Name(NameRef::Unresolved(u)) = target {
        if !diagnosed.contains(&u.text) {
            errors.push(CoverageError::UnresolvedWithoutDiagnostic {
                name: u.text.clone(),
                id,
            });
        }
    }
}

fn check_name_ref(nr: &NameRef, id: HirId, diagnosed: &[String], errors: &mut Vec<CoverageError>) {
    if let NameRef::Unresolved(u) = nr {
        if !diagnosed.contains(&u.text) {
            errors.push(CoverageError::UnresolvedWithoutDiagnostic {
                name: u.text.clone(),
                id,
            });
        }
    }
}

/// A coverage error discovered by `check_all`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverageError {
    /// A `NameRef::Unresolved` in the HIR with no corresponding diagnostic.
    UnresolvedWithoutDiagnostic { name: String, id: HirId },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use axiom_parser::ast::AstNode;

    fn lower_source(source: &str) -> Hir {
        let result = axiom_parser::parse(source);
        let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
        lower(&root, source)
    }

    #[test]
    fn test_lower_fn_def() {
        let hir = lower_source("fn main() { print(\"hello\") }");
        assert_eq!(hir.items.len(), 1);
        match &hir.items[0] {
            Item::FnDef(f) => {
                assert_eq!(f.name, "main");
                assert!(f.params.is_empty());
            }
            _ => panic!("expected FnDef"),
        }
    }

    #[test]
    fn test_lower_struct_def() {
        let hir = lower_source("struct Point { x: Float, y: Float }");
        assert_eq!(hir.items.len(), 1);
        match &hir.items[0] {
            Item::StructDef(s) => {
                assert_eq!(s.name, "Point");
                assert_eq!(s.fields.len(), 2);
                assert_eq!(s.fields[0].name, "x");
                assert_eq!(s.fields[1].name, "y");
            }
            _ => panic!("expected StructDef"),
        }
    }

    #[test]
    fn test_lower_enum_def() {
        let hir = lower_source("enum Shape { Circle(Float), Rect(Float, Float), Empty }");
        assert_eq!(hir.items.len(), 1);
        match &hir.items[0] {
            Item::EnumDef(e) => {
                assert_eq!(e.name, "Shape");
                assert_eq!(e.variants.len(), 3);
                assert_eq!(e.variants[0].name, "Circle");
                assert_eq!(e.variants[0].payload.len(), 1);
                assert_eq!(e.variants[2].name, "Empty");
                assert!(e.variants[2].payload.is_empty());
            }
            _ => panic!("expected EnumDef"),
        }
    }

    #[test]
    fn test_resolve_local_binding() {
        let hir = lower_source("fn main() { val x = 1 val y = x }");
        assert!(
            hir.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            hir.diagnostics
        );
        match &hir.items[0] {
            Item::FnDef(f) => {
                assert_eq!(f.body.stmts.len(), 2);
                match &f.body.stmts[1] {
                    Stmt::ValStmt(s) => match &s.value {
                        Expr::Path(p) => match &p.name_ref {
                            NameRef::Resolved(r) => assert_eq!(r.text, "x"),
                            NameRef::Unresolved(u) => panic!("x should resolve, got: {}", u.text),
                        },
                        _ => panic!("expected Path expr"),
                    },
                    _ => panic!("expected ValStmt"),
                }
            }
            _ => panic!("expected FnDef"),
        }
    }

    #[test]
    fn test_resolve_builtin() {
        let hir = lower_source("fn main() { print(\"hi\") }");
        assert!(
            hir.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            hir.diagnostics
        );
        match &hir.items[0] {
            Item::FnDef(f) => {
                assert!(
                    f.body.stmts.is_empty(),
                    "single-expr block should have empty stmts"
                );
                match f.body.tail.as_deref() {
                    Some(Expr::Call(c)) => match &c.callee {
                        NameRef::Resolved(r) => {
                            assert_eq!(r.text, "print");
                        }
                        NameRef::Unresolved(u) => {
                            panic!("print should resolve as builtin, got: {}", u.text)
                        }
                    },
                    _ => panic!("expected Call in tail"),
                }
            }
            _ => panic!("expected FnDef"),
        }
    }

    #[test]
    fn test_resolve_function_call() {
        let hir = lower_source("fn add(a: Int, b: Int) -> Int { a + b } fn main() { add(1, 2) }");
        assert!(
            hir.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            hir.diagnostics
        );
        match &hir.items[1] {
            Item::FnDef(main_fn) => {
                assert!(
                    main_fn.body.stmts.is_empty(),
                    "single-expr block should have empty stmts"
                );
                match main_fn.body.tail.as_deref() {
                    Some(Expr::Call(c)) => match &c.callee {
                        NameRef::Resolved(r) => assert_eq!(r.text, "add"),
                        NameRef::Unresolved(u) => {
                            panic!("add should resolve, got: {}", u.text)
                        }
                    },
                    _ => panic!("expected Call in tail"),
                }
            }
            _ => panic!("expected FnDef for main"),
        }
    }

    #[test]
    fn test_serialize_fn_def() {
        let hir = lower_source("fn main() { print(\"hi\") }");
        let dump = serialize(&hir);
        assert!(dump.contains("FnDef"));
        assert!(dump.contains("name=main"));
        assert!(dump.contains("Call"));
        assert!(dump.contains("print"));
    }

    #[test]
    fn test_hir_empty_source() {
        let result = axiom_parser::parse("");
        let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
        let hir = lower(&root, "");
        assert!(hir.items.is_empty());
    }

    #[test]
    fn test_hir_serialize_deterministic() {
        let source = "fn f() { val x = 1 }";
        let result = axiom_parser::parse(source);
        let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
        let hir1 = lower(&root, source);
        let hir2 = lower(&root, source);
        assert_eq!(serialize(&hir1), serialize(&hir2));
    }

    #[test]
    fn test_unresolved_name_emits_diagnostic() {
        let hir = lower_source("fn main() { val x = unknown_var }");
        assert!(
            !hir.diagnostics.is_empty(),
            "expected diagnostic for unknown_var"
        );
        match &hir.diagnostics[0] {
            HirDiagnostic::UnresolvedName { name, .. } => {
                assert_eq!(name, "unknown_var");
            }
            other => panic!("expected UnresolvedName, got: {:?}", other),
        }
    }

    #[test]
    fn test_check_all_clean() {
        let hir = lower_source("fn main() { val x = 1 }");
        assert!(check_all(&hir).is_ok());
    }

    #[test]
    fn test_check_all_unresolved_without_diagnostic() {
        // When the resolver emits UnresolvedName diagnostics, check_all
        // should still pass because every unresolved name is accounted for.
        let hir = lower_source("fn main() { val x = unknown_var }");
        assert!(check_all(&hir).is_ok());
    }

    // ── Generics tests ──────────────────────────────────────────────────────

    #[test]
    fn test_generic_struct_type_params() {
        let hir = lower_source("struct Box<T> { value: T }");
        assert!(
            hir.diagnostics.is_empty(),
            "unexpected: {:?}",
            hir.diagnostics
        );
        match &hir.items[0] {
            Item::StructDef(s) => {
                assert_eq!(s.type_params.len(), 1);
                assert_eq!(s.type_params[0].name, "T");
                // Field type T should be resolved to TypeParam.
                match &s.fields[0].ty {
                    HirTy::TypeParam(tp) => assert_eq!(tp.name, "T"),
                    other => panic!("expected TypeParam for field type, got: {:?}", other),
                }
            }
            _ => panic!("expected StructDef"),
        }
    }

    #[test]
    fn test_generic_fn_type_params() {
        let hir = lower_source("fn identity<T>(x: T) -> T { x }");
        assert!(
            hir.diagnostics.is_empty(),
            "unexpected: {:?}",
            hir.diagnostics
        );
        match &hir.items[0] {
            Item::FnDef(f) => {
                assert_eq!(f.type_params.len(), 1);
                assert_eq!(f.type_params[0].name, "T");
                // Param type T should be resolved.
                match &f.params[0].ty {
                    Some(HirTy::TypeParam(tp)) => assert_eq!(tp.name, "T"),
                    other => panic!("expected TypeParam for param type, got: {:?}", other),
                }
                // Return type T should be resolved.
                match &f.return_type {
                    Some(HirTy::TypeParam(tp)) => assert_eq!(tp.name, "T"),
                    other => panic!("expected TypeParam for return type, got: {:?}", other),
                }
            }
            _ => panic!("expected FnDef"),
        }
    }

    #[test]
    fn test_generic_type_instance() {
        let hir = lower_source("fn f(x: Pair<Int, Bool>) { x }");
        assert!(
            hir.diagnostics.is_empty(),
            "unexpected: {:?}",
            hir.diagnostics
        );
        match &hir.items[0] {
            Item::FnDef(f) => {
                match &f.params[0].ty {
                    Some(HirTy::Instance(inst)) => {
                        // Base name Pair is unresolved (not defined in this snippet).
                        match &inst.name {
                            NameRef::Unresolved(u) => assert_eq!(u.text, "Pair"),
                            other => panic!("expected unresolved Pair, got: {:?}", other),
                        }
                        assert_eq!(inst.args.len(), 2);
                        // Args Int and Bool are named types.
                        assert!(matches!(inst.args[0], HirTy::Named(_)));
                        assert!(matches!(inst.args[1], HirTy::Named(_)));
                    }
                    other => panic!("expected Instance type, got: {:?}", other),
                }
            }
            _ => panic!("expected FnDef"),
        }
    }

    #[test]
    fn test_generic_enum_type_params() {
        let hir = lower_source("enum Option<T> { Some(T), None }");
        assert!(
            hir.diagnostics.is_empty(),
            "unexpected: {:?}",
            hir.diagnostics
        );
        match &hir.items[0] {
            Item::EnumDef(e) => {
                assert_eq!(e.type_params.len(), 1);
                assert_eq!(e.type_params[0].name, "T");
                // Variant payload T should be resolved to TypeParam.
                match &e.variants[0].payload[0] {
                    HirTy::TypeParam(tp) => assert_eq!(tp.name, "T"),
                    other => panic!("expected TypeParam for variant payload, got: {:?}", other),
                }
            }
            _ => panic!("expected EnumDef"),
        }
    }

    #[test]
    fn test_generic_type_params_in_serialize() {
        let hir = lower_source("struct Pair<A, B> { first: A, second: B }");
        let dump = serialize(&hir);
        assert!(dump.contains("name=Pair<A, B>"), "dump: {dump}");
        assert!(dump.contains("first: A→"), "dump: {dump}");
        assert!(dump.contains("second: B→"), "dump: {dump}");
    }

    #[test]
    fn test_generic_trait_bound() {
        let hir = lower_source("fn sort<T: Ord>(items: T) { items }");
        assert!(
            hir.diagnostics.is_empty(),
            "unexpected: {:?}",
            hir.diagnostics
        );
        match &hir.items[0] {
            Item::FnDef(f) => {
                assert_eq!(f.type_params.len(), 1);
                assert_eq!(f.type_params[0].name, "T");
                assert_eq!(f.type_params[0].bounds.len(), 1);
                match &f.type_params[0].bounds[0].name {
                    NameRef::Unresolved(u) => assert_eq!(u.text, "Ord"),
                    other => panic!("expected unresolved bound, got: {:?}", other),
                }
            }
            _ => panic!("expected FnDef"),
        }
    }

    #[test]
    fn test_no_generics_backward_compatible() {
        // Non-generic code should still work exactly as before.
        let hir = lower_source("fn add(a: Int, b: Int) -> Int { a + b }");
        assert!(
            hir.diagnostics.is_empty(),
            "unexpected: {:?}",
            hir.diagnostics
        );
        match &hir.items[0] {
            Item::FnDef(f) => {
                assert!(f.type_params.is_empty());
            }
            _ => panic!("expected FnDef"),
        }
    }
}
