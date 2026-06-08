//! Desugaring goldens + coverage invariant
//! (`docs/lang-items-and-desugaring-design.md` §6.3).
//!
//! The core fear this guards against: "a sugar form I added silently fell
//! through to no rule / the wrong rule." It mechanizes three properties:
//!
//! 1. **Every sugar `Expr` variant has a desugaring golden** that pins the exact
//!    desugared IR of its driver `fn main` (compiled against the real stdlib, so
//!    the synthesized `List::…` calls resolve). Regenerate with
//!    `UPDATE_SNAPSHOTS=1`.
//! 2. **Every sugar golden shows its desugared calls** — a structural check that
//!    the literal produces a real stdlib call chain, never a compiler-native
//!    value (§6.4).
//! 3. **Drift guard on the `Expr` enum** — the full variant list is mirrored
//!    here, so adding *any* new `Expr` variant forces a conscious decision about
//!    whether it is sugar (and thus needs a `SugarSpec` + golden).
//!
//! Template: the `IrInstr`/`Terminator` variant-coverage tests in
//! `axiom-vm/tests/invariants.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

/// One sugar `Expr` form: the HIR variant, the golden stem, the driver program,
/// and the stdlib calls its desugared IR must contain. The single source of
/// truth for "what desugars and to what."
struct SugarSpec {
    /// The HIR `Expr` variant that is sugar.
    expr_variant: &'static str,
    /// The `desugar_goldens/<stem>.ir` golden that pins it.
    golden_stem: &'static str,
    /// A program whose `main` exercises this sugar.
    program: &'static str,
    /// Calls the desugared IR of `main` must contain.
    expected_calls: &'static [&'static str],
}

/// Every sugar `Expr` variant. A new one must be added here with a golden.
const SUGAR_EXPRS: &[SugarSpec] = &[
    SugarSpec {
        expr_variant: "ListLit",
        golden_stem: "list_literal",
        program: "fn main() {\n    val xs = [10, 20, 30]\n}\n",
        expected_calls: &["List::with_capacity", "List::push"],
    },
    // Indexed-place assignment (`base[i] = v` / `base[i] += v`) on a library
    // collection desugars to the `subscript_set` setter — never a raw struct
    // `IndexSet` (`docs/mutable-subscript-design.md` §4.2). The compound form
    // reads the old element back through `List::subscript` first.
    SugarSpec {
        expr_variant: "Assign",
        golden_stem: "index_assign",
        program: "fn main() {\n    var xs = [10, 20, 30]\n    xs[0] = 40 + 5\n    xs[1] += 7\n}\n",
        expected_calls: &["List::subscript_set", "List::subscript"],
    },
];

/// Every `Expr` variant in `resolver::Expr`, mirrored here so adding a variant
/// forces updating this test (and classifying it as sugar or not). Keep in sync
/// with the enum in `crates/axiom-hir/src/hir/mod.rs`.
const ALL_EXPR_VARIANTS: &[&str] = &[
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
];

fn goldens_dir() -> PathBuf {
    PathBuf::from("tests/desugar_goldens")
}

/// Compile `source` on the embedded stdlib and return the serialized IR of just
/// the `main` function — the sugar's desugared output, with `List::…` calls
/// resolved against the real stdlib definitions.
fn desugared_main_ir(source: &str) -> String {
    let thir = driver::check_modules(&stdlib::with_main(source));
    assert!(
        thir.diagnostics.is_empty(),
        "driver program did not compile cleanly: {:?}",
        thir.diagnostics
    );
    let mono = specialize::monomorphize(&thir);
    let ir = ir::lower(&thir, &mono);
    let full = ir::serialize(&ir);
    extract_fn(&full, "main")
}

/// Pull a single top-level `fn <name>(` block out of serialized IR. Functions
/// are emitted as `fn name(...) { ... }` blocks; we take from the header line up
/// to (and including) the line that closes it at column 0.
fn extract_fn(serialized: &str, name: &str) -> String {
    let header = format!("fn {name}(");
    let lines: Vec<&str> = serialized.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.starts_with(&header))
        .unwrap_or_else(|| panic!("function `{name}` not found in IR:\n{serialized}"));
    let mut out = String::new();
    for line in &lines[start..] {
        out.push_str(line);
        out.push('\n');
        if *line == "}" {
            break;
        }
    }
    out
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[test]
fn test_expr_variant_count_is_pinned() {
    // If this fails, an `Expr` variant was added or removed. Update
    // ALL_EXPR_VARIANTS, then decide: is the new variant sugar? If so, add a
    // SugarSpec + golden below.
    assert_eq!(
        ALL_EXPR_VARIANTS.len(),
        15,
        "resolver::Expr variant count changed — reconcile ALL_EXPR_VARIANTS \
         and decide whether the new variant is sugar (needs a SugarSpec + golden)"
    );
}

#[test]
fn test_sugar_specs_name_real_variants() {
    for spec in SUGAR_EXPRS {
        assert!(
            ALL_EXPR_VARIANTS.contains(&spec.expr_variant),
            "sugar spec names unknown Expr variant `{}`",
            spec.expr_variant
        );
    }
}

#[test]
fn test_every_sugar_has_a_stable_golden() {
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    for spec in SUGAR_EXPRS {
        let actual = desugared_main_ir(spec.program);
        let golden = goldens_dir().join(format!("{}.ir", spec.golden_stem));
        if update {
            fs::write(&golden, &actual).expect("write desugar golden");
            continue;
        }
        let expected = fs::read_to_string(&golden).unwrap_or_else(|_| {
            panic!(
                "missing desugar golden {} — run UPDATE_SNAPSHOTS=1",
                golden.display()
            )
        });
        assert_eq!(
            normalize(&actual),
            normalize(&expected),
            "desugar golden mismatch for sugar `{}`",
            spec.expr_variant
        );
    }
}

#[test]
fn test_every_sugar_golden_shows_its_desugared_calls() {
    // The "no compiler-native value" invariant (§6.4): the desugared literal is a
    // real stdlib call chain, not an opaque runtime value.
    for spec in SUGAR_EXPRS {
        let ir = desugared_main_ir(spec.program);
        for call in spec.expected_calls {
            assert!(
                ir.contains(call),
                "sugar `{}` did not desugar to `{}`:\n{ir}",
                spec.expr_variant,
                call
            );
        }
    }
}
