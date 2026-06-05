//! Fuzz/property tests for the type checker.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_hir::lower;
use axiom_parser::ast::AstNode;
use axiom_typeck::{check, serialize};

/// Property: the type checker never panics on any input that successfully parses.
#[test]
fn test_typeck_no_panic_on_well_formed() {
    let sources = [
        "fn main() { }",
        "fn main() { val x = 1 }",
        "fn main() { val x = 1 val y = 2 val z = x + y }",
        "fn f() -> Int { 42 }",
        "struct Point { x: Float, y: Float }",
        "enum Color { Red, Green, Blue }",
        "fn main() { if true { val x = 1 } }",
        "fn main() { loop { print(\"hello\") } }",
        "fn main() { val x: Bool = true }",
    ];
    for source in sources {
        let result = axiom_parser::parse(source);
        let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
        let hir = lower(&root, source, None);
        let _ = check(hir); // Must not panic.
    }
}

/// Property: the THIR dump is deterministic — same input, same output.
#[test]
fn test_typeck_deterministic() {
    let source = "fn add(a: Int, b: Int) -> Int { a + b }";
    let result1 = {
        let result = axiom_parser::parse(source);
        let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
        let hir = lower(&root, source, None);
        let thir = check(hir);
        serialize(&thir, None)
    };
    let result2 = {
        let result = axiom_parser::parse(source);
        let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
        let hir = lower(&root, source, None);
        let thir = check(hir);
        serialize(&thir, None)
    };
    assert_eq!(result1, result2, "THIR dump must be deterministic");
}

/// Property: TypeDiagnostic::kind() returns a non-empty string for every variant.
#[test]
fn test_diagnostic_kinds_non_empty() {
    use axiom_lexer::Span;
    let s = Span { lo: 0, hi: 0 };
    let diagnostics = all_diagnostic_variants(s);
    for diag in &diagnostics {
        assert!(
            !diag.kind().is_empty(),
            "diagnostic kind should not be empty"
        );
    }
}

#[allow(clippy::too_many_lines)]
fn all_diagnostic_variants(s: axiom_lexer::Span) -> Vec<axiom_typeck::TypeDiagnostic> {
    use axiom_typeck::TypeDiagnostic;
    vec![
        TypeDiagnostic::TypeMismatch {
            expected: "A".into(),
            found: "B".into(),
            span: s,
        },
        TypeDiagnostic::UndefinedType {
            name: "X".into(),
            span: s,
        },
        TypeDiagnostic::UnknownField {
            field: "f".into(),
            ty: "S".into(),
            span: s,
        },
        TypeDiagnostic::UnknownVariant {
            variant: "V".into(),
            name: "E".into(),
            span: s,
        },
        TypeDiagnostic::CallArityMismatch {
            name: "f".into(),
            expected: 1,
            found: 2,
            span: s,
        },
        TypeDiagnostic::StructFieldCountMismatch {
            name: "S".into(),
            expected: 2,
            found: 1,
            span: s,
        },
        TypeDiagnostic::StructMissingField {
            name: "S".into(),
            field: "x".into(),
            span: s,
        },
        TypeDiagnostic::StructUnknownField {
            name: "S".into(),
            field: "y".into(),
            span: s,
        },
        TypeDiagnostic::NonExhaustiveMatch {
            missing: vec![],
            span: s,
        },
        TypeDiagnostic::MatchArmTypeMismatch {
            expected: "A".into(),
            found: "B".into(),
            arm_index: 0,
            span: s,
        },
        TypeDiagnostic::IfBranchMismatch {
            expected: "A".into(),
            found: "B".into(),
            span: s,
        },
        TypeDiagnostic::LoopBodyNotUnit {
            found: "Int".into(),
            span: s,
        },
        TypeDiagnostic::ConditionNotBool {
            found: "Int".into(),
            span: s,
        },
        TypeDiagnostic::NotCallable {
            name: "x".into(),
            found: "Int".into(),
            span: s,
        },
        TypeDiagnostic::BinOpMismatch {
            op: "+".into(),
            left: "Int".into(),
            right: "String".into(),
            span: s,
        },
        TypeDiagnostic::UnaryOpMismatch {
            op: "-".into(),
            operand: "Bool".into(),
            span: s,
        },
        TypeDiagnostic::AssignToImmutable {
            name: "x".into(),
            span: s,
        },
        TypeDiagnostic::ReturnTypeMismatch {
            expected: "Int".into(),
            found: "String".into(),
            span: s,
        },
        TypeDiagnostic::NotYetSupported {
            feature: "X".into(),
            span: s,
        },
        TypeDiagnostic::BreakTypeMismatch {
            expected: "Int".into(),
            found: "String".into(),
            span: s,
        },
    ]
}
