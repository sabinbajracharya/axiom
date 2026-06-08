//! End-to-end: `string::format` flows parse → typeck → IR → VM and renders its
//! arguments at runtime. The result feeds the String-only `print`. See
//! `docs/string-format-and-print-retire.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

/// Run a program through the full pipeline and return the concatenated output
/// the VM emitted (the `output` trace events).
fn run_output(source: &str) -> String {
    let thir = driver::check_modules(&stdlib::with_main(source));
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected type diagnostics: {:?}",
        thir.diagnostics
    );
    let mono = typecheck::monomorphize(&thir);
    let ir = ir::lower(&thir, &mono);
    let mut vm = vm::Vm::new(ir);
    vm.set_tracing(true);
    vm.run().expect("vm run");
    // The trace records each platform write; the rendered text appears in the
    // `[fn output] …` lines. Return the whole trace and let callers search it.
    vm.take_trace().map(|t| t.output()).unwrap_or_default()
}

#[test]
fn test_format_renders_int_through_print() {
    let out = run_output(r#"fn main() { print(string::format("answer = {}", 42)) }"#);
    assert!(out.contains("answer = 42"), "got: {out:?}");
}

#[test]
fn test_format_multiple_args_and_println() {
    let out = run_output(r#"fn main() { println(string::format("{} + {} = {}", 1, 2, 3)) }"#);
    assert!(out.contains("1 + 2 = 3"), "got: {out:?}");
}
