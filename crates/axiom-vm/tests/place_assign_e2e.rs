//! End-to-end: assignment to *places* — struct fields (`c.n = 9`) and indexed
//! elements (`xs[0] = 9`). Previously `resolve_assign_target` returned a
//! sentinel and these lowered to nothing. They are the foundation for the
//! mutating collection methods (List::push writes `self.buf[i]` and
//! `self.count`).
//!
//! Assertions use `trace.output()` (the program's *real* printed text), so a
//! silent no-op cannot pass on a coincidental trace substring — the gap that
//! hid the indexed-write bug (`docs/mutable-subscript-design.md` §6).

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
fn test_struct_field_assignment_runs() {
    let out = run_output(
        r#"struct Counter { n: Int }
fn main() {
    var c = Counter { n: 5 }
    c.n = 9
    print(format("{}", c.n))
}"#,
    );
    assert_eq!(out, "9", "got: {out:?}");
}

#[test]
fn test_struct_field_compound_assignment_runs() {
    let out = run_output(
        r#"struct Counter { n: Int }
fn main() {
    var c = Counter { n: 5 }
    c.n = c.n + 3
    print(format("{}", c.n))
}"#,
    );
    assert_eq!(out, "8", "got: {out:?}");
}

#[test]
fn test_list_index_assignment_runs() {
    let out = run_output(
        r#"fn main() {
    var xs = [1, 2, 3]
    xs[0] = 9
    print(format("{}", xs[0]))
}"#,
    );
    assert_eq!(out, "9", "got: {out:?}");
}
