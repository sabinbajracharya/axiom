//! End-to-end: the `[]` subscript operator on stdlib collections dispatches to
//! the type's `subscript` function (`List::subscript` / `Map::subscript`).
//!
//! Assertions use *computed* values (products/sums that never appear as
//! literals in the source) so a match proves the subscript actually returned
//! the stored element — not a coincidental hit on a constant in the trace.

#![allow(clippy::unwrap_used, clippy::expect_used)]

fn run_output(source: &str) -> String {
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

#[test]
fn test_list_subscript_reads_elements() {
    // push 3, 5, 7 → product 105 and sum-of-ends 10 are not literals in source.
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = List::new()
    xs.push(3)
    xs.push(5)
    xs.push(7)
    print(format("{}", xs[0] * xs[1] * xs[2]))
    print(format("{}", xs[0] + xs[2]))
}"#,
    );
    assert!(out.contains("105"), "expected product 105, got: {out:?}");
    assert!(out.contains("10"), "expected sum-of-ends 10, got: {out:?}");
}

#[test]
fn test_list_subscript_after_growth() {
    // Cross the initial capacity, then read a late element via subscript.
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = List::new()
    var i = 0
    loop if i < 8 {
        xs.push(i + 1)
        i = i + 1
    }
    print(format("{}", xs[7] * 100))
}"#,
    );
    // xs[7] == 8 → 800, a value that appears nowhere as a literal.
    assert!(
        out.contains("800"),
        "expected xs[7]*100 = 800, got: {out:?}"
    );
}

#[test]
fn test_map_subscript_reads_values() {
    // set a→3, b→5 → product 15 is not a literal in source.
    let out = run_output(
        r#"fn main() {
    var m: Map<String, Int> = Map::new()
    m.set("a", 3)
    m.set("b", 5)
    print(format("{}", m["a"] * m["b"]))
}"#,
    );
    assert!(out.contains("15"), "expected m[a]*m[b] = 15, got: {out:?}");
}
