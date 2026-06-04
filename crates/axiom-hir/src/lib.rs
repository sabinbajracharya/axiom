//! The Axiom HIR: a desugared, ID-keyed tree where every identifier resolves to a
//! binding or item def.
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
            Item::FnDef(f) => match &f.body.stmts[0] {
                Stmt::ExprStmt(s) => match &s.expr {
                    Expr::Call(c) => match &c.callee {
                        NameRef::Resolved(r) => {
                            assert_eq!(r.text, "print");
                        }
                        NameRef::Unresolved(u) => {
                            panic!("print should resolve as builtin, got: {}", u.text)
                        }
                    },
                    _ => panic!("expected Call"),
                },
                _ => panic!("expected ExprStmt"),
            },
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
            Item::FnDef(main_fn) => match &main_fn.body.stmts[0] {
                Stmt::ExprStmt(s) => match &s.expr {
                    Expr::Call(c) => match &c.callee {
                        NameRef::Resolved(r) => assert_eq!(r.text, "add"),
                        NameRef::Unresolved(u) => {
                            panic!("add should resolve, got: {}", u.text)
                        }
                    },
                    _ => panic!("expected Call"),
                },
                _ => panic!("expected ExprStmt"),
            },
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
}
