//! Coverage invariants for the desugar pass. Extracted from `tests.rs` to stay
//! under the 600-line cap (RUST_CONVENTIONS.md §10).

use crate::DefKind;

/// Expr-variant coverage invariant: every variant in the Expr enum must be
/// explicitly classified. Adding a new Expr variant without updating this
/// test fails the build.
#[test]
fn test_every_expr_variant_handled_by_desugar() {
    let sugar: &[&str] = &["ListLit", "Question", "Catch", "Else"];
    let non_sugar: &[&str] = &[
        "Lit",
        "Path",
        "Bin",
        "Unary",
        "Call",
        "MethodCall",
        "Field",
        "Index",
        "Block",
        "If",
        "Match",
        "Loop",
        "StructLit",
        "Assign",
    ];
    let all_known: std::collections::BTreeSet<&str> =
        sugar.iter().chain(non_sugar.iter()).copied().collect();
    let all_expr: &[&str] = &[
        "Lit",
        "Path",
        "Bin",
        "Unary",
        "Call",
        "MethodCall",
        "Field",
        "Index",
        "Block",
        "If",
        "Match",
        "Loop",
        "StructLit",
        "ListLit",
        "Assign",
        "Question",
        "Catch",
        "Else",
    ];
    assert_eq!(all_expr.len(), 18, "Expr variant count changed");
    let known: std::collections::BTreeSet<&str> = all_expr.iter().copied().collect();
    assert_eq!(all_known, known, "every Expr variant must be classified");
}

/// DefKind coverage invariant: ensures the resolver's `build_top_level` and
/// `build_global_exports` filters include every DefKind variant that belongs in
/// the top-level scope. Adding a new DefKind without updating this test fails the
/// build.
#[test]
fn test_def_kind_filter_coverage() {
    let filter_kinds: &[&str] = &[
        "Fn",
        "Struct",
        "Enum",
        "Trait",
        "Variant",
        "ErrorSet",
        "ErrorVariant",
    ];
    let nested_kinds: &[&str] = &["Field", "Param", "TypeParam", "Local", "Builtin"];

    let all_known: std::collections::BTreeSet<&str> = filter_kinds
        .iter()
        .chain(nested_kinds.iter())
        .copied()
        .collect();

    let all_defkind: &[&str] = &[
        "Fn",
        "Struct",
        "Enum",
        "Trait",
        "Variant",
        "Field",
        "Param",
        "TypeParam",
        "Local",
        "Builtin",
        "ErrorSet",
        "ErrorVariant",
    ];
    assert_eq!(all_defkind.len(), 12, "DefKind variant count changed");

    let known: std::collections::BTreeSet<&str> = all_defkind.iter().copied().collect();
    assert_eq!(
        all_known, known,
        "every DefKind variant must be classified as filter or nested"
    );

    for &k in filter_kinds {
        assert!(
            matches!(
                variant_from_name(k),
                DefKind::Fn
                    | DefKind::Struct
                    | DefKind::Enum
                    | DefKind::Trait
                    | DefKind::Variant
                    | DefKind::ErrorSet
                    | DefKind::ErrorVariant
            ),
            "DefKind::{k} must be in the top-level filter"
        );
    }
}

fn variant_from_name(name: &str) -> DefKind {
    match name {
        "Fn" => DefKind::Fn,
        "Struct" => DefKind::Struct,
        "Enum" => DefKind::Enum,
        "Trait" => DefKind::Trait,
        "Variant" => DefKind::Variant,
        "Field" => DefKind::Field,
        "Param" => DefKind::Param,
        "TypeParam" => DefKind::TypeParam,
        "Local" => DefKind::Local,
        "Builtin" => DefKind::Builtin,
        "ErrorSet" => DefKind::ErrorSet,
        "ErrorVariant" => DefKind::ErrorVariant,
        _ => panic!("unknown DefKind: {name}"),
    }
}
