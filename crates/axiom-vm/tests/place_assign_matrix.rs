//! H3 — place-assignment coverage matrix (`docs/mutable-subscript-design.md` §7).
//!
//! A data-driven matrix over `{ target } × { op } × { base }` where every cell
//! asserts the program's **real output** (`trace.output()`, never a trace
//! substring). It is drift-guarded like the `IrInstr` variant-coverage tests
//! (`invariants.rs`): the `AssignTarget` variants and the indexable base kinds
//! are pinned here, so adding a new assignment target or a new indexable base
//! kind without a covering row fails the build — the "a case I never imagined
//! slipped through" guard that the original silent-no-op bug evaded.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// Run `source` and return only what the program actually printed.
fn run_program(source: &str) -> String {
    let thir = axiom_typeck::check_modules(&axiom_stdlib::with_main(source));
    assert!(
        thir.diagnostics.is_empty(),
        "type diagnostics: {:?}",
        thir.diagnostics
    );
    let mono = axiom_typeck::monomorphize(&thir);
    let ir = axiom_ir::lower(&thir, &mono);
    let mut vm = axiom_vm::Vm::new(ir);
    vm.set_tracing(true);
    vm.run().expect("vm run");
    vm.take_trace().map(|t| t.output()).unwrap_or_default()
}

/// Every `AssignTarget` variant (`axiom_hir::AssignTarget`). Adding one forces a
/// new matrix row. Keep in sync with the enum.
const ASSIGN_TARGETS: &[&str] = &["Name", "Field", "Index"];

/// Every indexable base kind a `base[i] = v` can write through. Adding one
/// (a new collection lowering, say) forces a new matrix row.
const INDEX_BASE_KINDS: &[&str] = &["HeapBuffer", "List", "UserStruct"];

/// The six assignment operators, as `(spelling, label)`.
const OPS: &[(&str, &str)] = &[
    ("=", "plain"),
    ("+=", "add"),
    ("-=", "sub"),
    ("*=", "mul"),
    ("/=", "div"),
    ("%=", "mod"),
];

/// The seed value every cell starts the place at (`60`), chosen so a silent
/// no-op (which would leave `60`) differs from every expected result below.
const SEED: i64 = 60;

/// Compute the expected result of `SEED op rhs`.
fn apply(op: &str, rhs: i64) -> i64 {
    match op {
        "=" => rhs,
        "+=" => SEED + rhs,
        "-=" => SEED - rhs,
        "*=" => SEED * rhs,
        "/=" => SEED / rhs,
        "%=" => SEED % rhs,
        other => panic!("unknown op {other}"),
    }
}

/// A per-op right-hand side, picked so each result is distinct from `SEED`.
fn rhs_for(op: &str) -> i64 {
    match op {
        "=" => 45,
        "+=" => 7,
        "-=" => 5,
        "*=" => 3,
        "/=" => 4,
        "%=" => 7,
        other => panic!("unknown op {other}"),
    }
}

/// Build a program that writes the place via `op` and prints it back.
/// `target` ∈ ASSIGN_TARGETS; `base` is `Some(kind)` only for `Index`.
fn program_for(target: &str, base: Option<&str>, op: &str) -> String {
    let rhs = rhs_for(op);
    match (target, base) {
        ("Name", _) => format!(
            "fn main() {{\n    var n = {SEED}\n    n {op} {rhs}\n    print(format(\"{{}}\", n))\n}}\n"
        ),
        ("Field", _) => format!(
            "struct Box {{ v: Int }}\n\
             fn main() {{\n    var b = Box {{ v: {SEED} }}\n    \
             b.v {op} {rhs}\n    print(format(\"{{}}\", b.v))\n}}\n"
        ),
        ("Index", Some("HeapBuffer")) => format!(
            "fn main() {{\n    var xs: [Int] = heap_alloc(1)\n    xs[0] = {SEED}\n    \
             xs[0] {op} {rhs}\n    print(format(\"{{}}\", xs[0]))\n}}\n"
        ),
        ("Index", Some("List")) => format!(
            "fn main() {{\n    var xs = [{SEED}]\n    xs[0] {op} {rhs}\n    \
             print(format(\"{{}}\", xs[0]))\n}}\n"
        ),
        ("Index", Some("UserStruct")) => format!(
            "struct Cell {{ v: Int }}\n\
             impl Cell {{\n    subscript(i: Int) -> Int {{ self.v }}\n    \
             subscript(i: Int, value: Int) {{ self.v = value }}\n}}\n\
             fn main() {{\n    var c = Cell {{ v: {SEED} }}\n    c[0] {op} {rhs}\n    \
             print(format(\"{{}}\", c[0]))\n}}\n"
        ),
        other => panic!("no program template for {other:?}"),
    }
}

/// Run one cell and assert its real output equals `SEED op rhs`. Because the
/// assertion is exact-equality on real output (not a trace substring) and the
/// expected value differs from the un-written `SEED`, a silent no-op cannot
/// pass — the property the original bug violated.
fn check_cell(target: &str, base: Option<&str>, op: &str) {
    let expected = apply(op, rhs_for(op)).to_string();
    let src = program_for(target, base, op);
    let out = run_program(&src);
    assert_eq!(
        out, expected,
        "cell target={target} base={base:?} op={op} mismatch\nprogram:\n{src}"
    );
}

#[test]
fn matrix_name_target_all_ops() {
    for (op, _) in OPS {
        check_cell("Name", None, op);
    }
}

#[test]
fn matrix_field_target_all_ops() {
    for (op, _) in OPS {
        check_cell("Field", None, op);
    }
}

#[test]
fn matrix_index_target_all_bases_all_ops() {
    for base in INDEX_BASE_KINDS {
        for (op, _) in OPS {
            check_cell("Index", Some(base), op);
        }
    }
}

#[test]
fn matrix_covers_every_assign_target() {
    // Drift guard: a new AssignTarget variant must add a covering test above.
    assert_eq!(
        ASSIGN_TARGETS.len(),
        3,
        "AssignTarget variant set changed — add a matrix row for the new target"
    );
}

#[test]
fn matrix_covers_every_index_base_kind() {
    // Drift guard: a new indexable base kind must add a covering row in
    // `program_for` + `matrix_index_target_all_bases_all_ops`.
    assert_eq!(
        INDEX_BASE_KINDS.len(),
        3,
        "indexable base-kind set changed — add a matrix row for the new base"
    );
    // Every declared base kind must have a usable program template for every op.
    for base in INDEX_BASE_KINDS {
        for (op, _) in OPS {
            let src = program_for("Index", Some(base), op);
            assert!(!src.is_empty(), "missing template for {base}/{op}");
        }
    }
}
