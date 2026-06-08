//! End-to-end: String::len is library code (core/string.ax) calling
//! self.as_bytes().len() — the Bytes::len floor.
#![allow(clippy::unwrap_used, clippy::expect_used)]

fn run_output(source: &str) -> String {
    let thir = driver::check_modules(&stdlib::with_main(source));
    assert!(
        thir.diagnostics.is_empty(),
        "diagnostics: {:?}",
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
fn test_string_len_runs() {
    let out = run_output(r#"fn main() { print(string::format("{}", ("hello").len())) }"#);
    assert!(out.contains('5'), "got: {out:?}");
}

#[test]
fn test_string_len_empty_runs() {
    let out = run_output(r#"fn main() { print(string::format("{}", ("").len())) }"#);
    assert!(out.contains('0'), "got: {out:?}");
}
