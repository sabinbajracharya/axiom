//! End-to-end: calling a trait method on a bounded type parameter
//! (`x.hash()` where `x: T, T: Hashable`) resolves through the bound in typeck
//! and dispatches to the concrete impl after monomorphization. This is the
//! mechanism `Map<K, V>` relies on to hash its keys (M7).

#![allow(clippy::unwrap_used, clippy::expect_used)]

fn run_output(source: &str) -> String {
    let thir = driver::check_modules(&stdlib::with_main(source));
    assert!(
        thir.diagnostics.is_empty(),
        "type diagnostics: {:?}",
        thir.diagnostics
    );
    let mono = typecheck::monomorphize(&thir);
    let ir = ir::lower(&thir, &mono);
    let mut vm = vm::Vm::new(ir);
    vm.set_tracing(true);
    vm.run().expect("vm run");
    vm.take_trace().map(|t| t.output()).unwrap_or_default()
}

#[test]
fn test_hash_on_bounded_type_param_dispatches_to_concrete_impl() {
    // `h(7)` monomorphizes for `T = Int`; `x.hash()` must dispatch to
    // `Int::hash` (which forwards to the `hash_raw` floor).
    let out = run_output(
        r#"fn h<T: Hashable>(x: T) -> Int { x.hash() }
fn main() {
    print(format("{}", h(7)))
}"#,
    );
    // Int::hash is deterministic; assert the call ran and produced an integer
    // (real program output, not a trace artifact).
    assert!(
        !out.is_empty() && out.parse::<i64>().is_ok(),
        "expected an integer hash as program output, got: {out:?}"
    );
}

#[test]
fn test_eq_on_type_param_returns_bool() {
    // `==` on two values of a type parameter type-checks structurally and
    // returns Bool — used by `Map` to compare keys.
    let out = run_output(
        r#"fn same<T: Equatable>(a: T, b: T) -> Bool { a == b }
fn main() {
    print(format("{}", same(3, 3)))
    print(format("{}", same(3, 4)))
}"#,
    );
    assert!(out.contains("true"), "got: {out:?}");
    assert!(out.contains("false"), "got: {out:?}");
}
