//! End-to-end: generic enums (`enum E<T> { ... }`) type-check and run —
//! constructor calls infer the type argument, and tuple-variant patterns bind
//! payloads at the substituted type. This is the foundation for `Option<T>`
//! (used by `Map::get`, M7).

#![allow(clippy::unwrap_used, clippy::expect_used)]

fn run_output(source: &str) -> String {
    let thir = axiom_driver::check_modules(&axiom_stdlib::with_main(source));
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
fn test_generic_enum_construct_and_match() {
    let out = run_output(
        r#"enum Maybe<T> { Just(T), Nothing }
fn main() {
    val a: Maybe<Int> = Maybe::Just(42)
    val r = match a {
        Just(x) => x,
        Nothing => 0,
    }
    print(format("{}", r))
}"#,
    );
    assert!(out.contains("42"), "got: {out:?}");
}

#[test]
fn test_generic_enum_nothing_branch() {
    let out = run_output(
        r#"enum Maybe<T> { Just(T), Nothing }
fn main() {
    val a: Maybe<Int> = Maybe::Nothing
    val r = match a {
        Just(x) => x,
        Nothing => 99,
    }
    print(format("{}", r))
}"#,
    );
    assert!(out.contains("99"), "got: {out:?}");
}
