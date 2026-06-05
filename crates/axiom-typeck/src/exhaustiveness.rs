//! Match exhaustiveness checking for enum types.
//!
//! Per `docs/typeck-testing.md` §9: when the scrutinee is a known enum type,
//! verify that patterns cover all variants. A `_` wildcard covers everything.
//! `OrPat` covers the union of its alternatives. Literal patterns need not
//! be exhaustive in v0 — only enum types require exhaustiveness checking.
//!
//! This module is a pure transform over patterns and variant lists,
//! testable in isolation without the full type checker.

use crate::error::TypeDiagnostic;
use axiom_hir::*;
use axiom_lexer::Span;

/// Check whether a match expression covers all variants of an enum.
/// Returns a `NonExhaustiveMatch` diagnostic if variants are uncovered,
/// or an empty list if the match is exhaustive.
///
/// `is_unit_variant` is a predicate that returns true if a pattern identifier
/// name refers to a unit variant (zero-parameter variant) of the enum being
/// matched. This allows the exhaustiveness checker to distinguish between
/// a unit variant match and a catch-all binding.
pub fn check_match_exhaustiveness(
    arms: &[MatchArm],
    all_variants: &[String],
    is_unit_variant: &impl Fn(&str) -> bool,
    span: Span,
) -> Vec<TypeDiagnostic> {
    if all_variants.is_empty() {
        return Vec::new();
    }

    let mut covered: Vec<String> = Vec::new();
    for arm in arms {
        // Guarded arms do not contribute to exhaustiveness — a guard is a
        // runtime predicate the compiler cannot evaluate statically. The arm
        // only covers its pattern when the guard is true, not universally.
        if arm.guard.is_some() {
            continue;
        }
        collect_covered_variants(&arm.pattern, all_variants, &mut covered, is_unit_variant);
    }

    let missing: Vec<String> = all_variants
        .iter()
        .filter(|v| !covered.contains(&v.to_string()))
        .cloned()
        .collect();

    if !missing.is_empty() {
        vec![TypeDiagnostic::NonExhaustiveMatch { missing, span }]
    } else {
        Vec::new()
    }
}

/// Collect the variant names covered by a pattern.
///
/// Wildcard and catch-all identifier patterns cover all variants.
/// TupleStruct and Struct patterns cover the named variant.
/// Or patterns cover the union of their alternatives.
/// Literal and Range patterns do not cover enum variants.
fn collect_covered_variants(
    pat: &Pattern,
    all_variants: &[String],
    covered: &mut Vec<String>,
    is_unit_variant: &impl Fn(&str) -> bool,
) {
    match pat {
        Pattern::Wildcard(_) => {
            covered.extend(all_variants.iter().cloned());
        }
        Pattern::Ident(p) => {
            if is_unit_variant(&p.name) {
                if !covered.contains(&p.name) {
                    covered.push(p.name.clone());
                }
            } else {
                covered.extend(all_variants.iter().cloned());
            }
        }
        Pattern::Literal(_) => {}
        Pattern::TupleStruct(ts) => match &ts.path {
            NameRef::Resolved(r) => {
                if !covered.contains(&r.text) {
                    covered.push(r.text.clone());
                }
            }
            NameRef::Unresolved(_) => {}
        },
        Pattern::Struct(sp) => match &sp.path {
            NameRef::Resolved(r) => {
                if !covered.contains(&r.text) {
                    covered.push(r.text.clone());
                }
            }
            NameRef::Unresolved(_) => {}
        },
        Pattern::Or(op) => {
            for alt in &op.alternatives {
                collect_covered_variants(alt, all_variants, covered, is_unit_variant);
            }
        }
        Pattern::Range(_) => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn make_match_arms(arms: Vec<Pattern>) -> Vec<MatchArm> {
        arms.into_iter()
            .map(|pattern| MatchArm {
                pattern,
                guard: None,
                body: Expr::Lit(LitExpr {
                    id: HirId(998),
                    kind: LitKind::Int(0),
                }),
            })
            .collect()
    }

    #[test]
    fn test_wildcard_covers_all() {
        let arms = make_match_arms(vec![Pattern::Wildcard(HirId(0))]);
        let diags =
            check_match_exhaustiveness(&arms, &["A".into(), "B".into()], &|_| false, span());
        assert!(diags.is_empty());
    }

    #[test]
    fn test_tuple_struct_covers_one() {
        let arms = make_match_arms(vec![Pattern::TupleStruct(TupleStructPat {
            id: HirId(0),
            path: NameRef::Resolved(ResolvedName {
                def_id: HirId(10),
                text: "A".into(),
            }),
            fields: vec![],
        })]);
        let diags =
            check_match_exhaustiveness(&arms, &["A".into(), "B".into()], &|_| false, span());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].to_string().contains("non-exhaustive"));
    }

    #[test]
    fn test_all_variants_covered() {
        let arms = make_match_arms(vec![
            Pattern::TupleStruct(TupleStructPat {
                id: HirId(0),
                path: NameRef::Resolved(ResolvedName {
                    def_id: HirId(10),
                    text: "A".into(),
                }),
                fields: vec![],
            }),
            Pattern::TupleStruct(TupleStructPat {
                id: HirId(1),
                path: NameRef::Resolved(ResolvedName {
                    def_id: HirId(11),
                    text: "B".into(),
                }),
                fields: vec![],
            }),
        ]);
        let diags =
            check_match_exhaustiveness(&arms, &["A".into(), "B".into()], &|_| false, span());
        assert!(diags.is_empty());
    }

    #[test]
    fn test_unit_variant_as_ident() {
        let arms = make_match_arms(vec![
            Pattern::Ident(IdentPat {
                id: HirId(0),
                name: "A".into(),
                binding: None,
                span: span(),
            }),
            Pattern::Ident(IdentPat {
                id: HirId(1),
                name: "B".into(),
                binding: None,
                span: span(),
            }),
        ]);
        let is_unit = |name: &str| name == "A" || name == "B";
        let diags = check_match_exhaustiveness(&arms, &["A".into(), "B".into()], &is_unit, span());
        assert!(diags.is_empty());
    }

    #[test]
    fn test_ident_catch_all_covers_all() {
        let arms = make_match_arms(vec![Pattern::Ident(IdentPat {
            id: HirId(0),
            name: "x".into(),
            binding: Some(HirId(1)),
            span: span(),
        })]);
        let diags =
            check_match_exhaustiveness(&arms, &["A".into(), "B".into()], &|_| false, span());
        assert!(diags.is_empty());
    }

    #[test]
    fn test_or_pattern_union() {
        let arms = make_match_arms(vec![Pattern::Or(OrPat {
            id: HirId(0),
            alternatives: vec![
                Pattern::TupleStruct(TupleStructPat {
                    id: HirId(1),
                    path: NameRef::Resolved(ResolvedName {
                        def_id: HirId(10),
                        text: "A".into(),
                    }),
                    fields: vec![],
                }),
                Pattern::TupleStruct(TupleStructPat {
                    id: HirId(2),
                    path: NameRef::Resolved(ResolvedName {
                        def_id: HirId(11),
                        text: "B".into(),
                    }),
                    fields: vec![],
                }),
            ],
        })]);
        let diags =
            check_match_exhaustiveness(&arms, &["A".into(), "B".into()], &|_| false, span());
        assert!(diags.is_empty());
    }

    // ── Guard × exhaustiveness tests ───────────────────────────────────────

    /// A guarded arm alone does NOT cover its variant — the guard is a
    /// runtime predicate the compiler cannot evaluate statically.
    #[test]
    fn test_guarded_arm_does_not_cover() {
        let arms = vec![MatchArm {
            pattern: Pattern::TupleStruct(TupleStructPat {
                id: HirId(0),
                path: NameRef::Resolved(ResolvedName {
                    def_id: HirId(10),
                    text: "A".into(),
                }),
                fields: vec![],
            }),
            guard: Some(Expr::Lit(LitExpr {
                id: HirId(99),
                kind: LitKind::Bool(true),
            })),
            body: Expr::Lit(LitExpr {
                id: HirId(998),
                kind: LitKind::Int(0),
            }),
        }];
        let diags =
            check_match_exhaustiveness(&arms, &["A".into(), "B".into()], &|_| false, span());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].to_string().contains("non-exhaustive"));
    }

    /// A guarded arm + a wildcard is exhaustive — the wildcard covers everything.
    #[test]
    fn test_guarded_arm_plus_wildcard_is_exhaustive() {
        let arms = vec![
            MatchArm {
                pattern: Pattern::TupleStruct(TupleStructPat {
                    id: HirId(0),
                    path: NameRef::Resolved(ResolvedName {
                        def_id: HirId(10),
                        text: "A".into(),
                    }),
                    fields: vec![],
                }),
                guard: Some(Expr::Lit(LitExpr {
                    id: HirId(99),
                    kind: LitKind::Bool(true),
                })),
                body: Expr::Lit(LitExpr {
                    id: HirId(998),
                    kind: LitKind::Int(0),
                }),
            },
            MatchArm {
                pattern: Pattern::Wildcard(HirId(1)),
                guard: None,
                body: Expr::Lit(LitExpr {
                    id: HirId(999),
                    kind: LitKind::Int(0),
                }),
            },
        ];
        let diags =
            check_match_exhaustiveness(&arms, &["A".into(), "B".into()], &|_| false, span());
        assert!(diags.is_empty());
    }

    /// A guarded arm + an unguarded arm for the same variant is exhaustive —
    /// the unguarded arm covers the variant; the guarded arm narrows it.
    #[test]
    fn test_guarded_plus_unguarded_same_variant_is_exhaustive() {
        let arms = vec![
            MatchArm {
                pattern: Pattern::TupleStruct(TupleStructPat {
                    id: HirId(0),
                    path: NameRef::Resolved(ResolvedName {
                        def_id: HirId(10),
                        text: "A".into(),
                    }),
                    fields: vec![],
                }),
                guard: Some(Expr::Lit(LitExpr {
                    id: HirId(99),
                    kind: LitKind::Bool(true),
                })),
                body: Expr::Lit(LitExpr {
                    id: HirId(998),
                    kind: LitKind::Int(1),
                }),
            },
            MatchArm {
                pattern: Pattern::TupleStruct(TupleStructPat {
                    id: HirId(1),
                    path: NameRef::Resolved(ResolvedName {
                        def_id: HirId(11),
                        text: "A".into(),
                    }),
                    fields: vec![],
                }),
                guard: None,
                body: Expr::Lit(LitExpr {
                    id: HirId(999),
                    kind: LitKind::Int(2),
                }),
            },
            MatchArm {
                pattern: Pattern::TupleStruct(TupleStructPat {
                    id: HirId(2),
                    path: NameRef::Resolved(ResolvedName {
                        def_id: HirId(12),
                        text: "B".into(),
                    }),
                    fields: vec![],
                }),
                guard: None,
                body: Expr::Lit(LitExpr {
                    id: HirId(1000),
                    kind: LitKind::Int(3),
                }),
            },
        ];
        let diags =
            check_match_exhaustiveness(&arms, &["A".into(), "B".into()], &|_| false, span());
        assert!(diags.is_empty());
    }

    /// Two guarded arms for all variants → still non-exhaustive.
    #[test]
    fn test_all_guarded_arms_still_non_exhaustive() {
        let arms = vec![
            MatchArm {
                pattern: Pattern::TupleStruct(TupleStructPat {
                    id: HirId(0),
                    path: NameRef::Resolved(ResolvedName {
                        def_id: HirId(10),
                        text: "A".into(),
                    }),
                    fields: vec![],
                }),
                guard: Some(Expr::Lit(LitExpr {
                    id: HirId(99),
                    kind: LitKind::Bool(true),
                })),
                body: Expr::Lit(LitExpr {
                    id: HirId(998),
                    kind: LitKind::Int(1),
                }),
            },
            MatchArm {
                pattern: Pattern::TupleStruct(TupleStructPat {
                    id: HirId(1),
                    path: NameRef::Resolved(ResolvedName {
                        def_id: HirId(11),
                        text: "B".into(),
                    }),
                    fields: vec![],
                }),
                guard: Some(Expr::Lit(LitExpr {
                    id: HirId(100),
                    kind: LitKind::Bool(true),
                })),
                body: Expr::Lit(LitExpr {
                    id: HirId(999),
                    kind: LitKind::Int(2),
                }),
            },
        ];
        let diags =
            check_match_exhaustiveness(&arms, &["A".into(), "B".into()], &|_| false, span());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].to_string().contains("non-exhaustive"));
    }

    fn span() -> Span {
        Span { lo: 0, hi: 0 }
    }
}
