//! End-to-end: the `inout` calling convention actually writes mutations back to
//! the caller. Previously calls passed args by value and only the return value
//! propagated, so `inout` was silently a no-op. This is the foundation for
//! mutating methods like `List::push(inout self, …)`.

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
    vm.take_trace().map(|t| t.format()).unwrap_or_default()
}

#[test]
fn test_inout_free_function_writes_back() {
    let out = run_output(
        r#"fn inc(inout x: Int) { x = x + 1 }
fn main() {
    var n = 5
    inc(n)
    print(format("{}", n))
}"#,
    );
    assert!(out.contains('6'), "got: {out:?}");
}

#[test]
fn test_inout_self_method_writes_back() {
    let out = run_output(
        r#"struct Counter { n: Int }
impl Counter {
    fn bump(inout self) { self.n = self.n + 1 }
    fn get(let self) -> Int { self.n }
}
fn main() {
    var c = Counter { n: 0 }
    c.bump()
    c.bump()
    print(format("{}", c.get()))
}"#,
    );
    assert!(out.contains('2'), "got: {out:?}");
}
