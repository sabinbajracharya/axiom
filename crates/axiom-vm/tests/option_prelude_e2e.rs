#![allow(clippy::unwrap_used, clippy::expect_used)]
fn run_output(source: &str) -> String {
    let thir = axiom_typeck::check_modules(&axiom_stdlib::with_main(source));
    assert!(thir.diagnostics.is_empty(), "diags: {:?}", thir.diagnostics);
    let mono = axiom_typeck::monomorphize(&thir);
    let ir = axiom_ir::lower(&thir, &mono);
    let mut vm = axiom_vm::Vm::new(ir);
    vm.set_tracing(true);
    vm.run().expect("vm run");
    vm.take_trace().map(|t| t.output()).unwrap_or_default()
}
#[test]
fn test_prelude_option() {
    let out = run_output(
        r#"fn main() {
    val a: Option<Int> = Option::Some(5)
    val r = match a { Some(x) => x, None => 0 }
    print(format("{}", r))
}"#,
    );
    assert!(out.contains('5'), "got: {out:?}");
}
#[test]
fn test_prelude_result() {
    let out = run_output(
        r#"fn main() {
    val a: Result<Int, String> = Result::Ok(42)
    val b: Result<Int, String> = Result::Err("boom")
    val ra = match a { Ok(x) => x, Err(_) => 0 }
    val rb = match b { Ok(x) => x, Err(_) => -1 }
    print(format("{} {}", ra, rb))
}"#,
    );
    assert!(out.contains("42 -1"), "got: {out:?}");
}
