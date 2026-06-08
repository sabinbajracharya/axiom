//! End-to-end: generic structs construct, type-check with inferred type
//! arguments, and run on the VM — including field access (`self.value`) and
//! generic method dispatch (`Box<Int>::get`). This is the D0 foundation that
//! the collection library types (List/Map) build on.

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
fn test_generic_struct_field_access_runs() {
    let out = run_output(
        r#"struct Box<T> { value: T }
impl<T> Box<T> {
    fn get(let self) -> T { self.value }
}
fn main() {
    val b = Box { value: 42 }
    print(format("{}", b.get()))
}"#,
    );
    assert!(out.contains("42"), "got: {out:?}");
}

#[test]
fn test_generic_struct_field_direct_runs() {
    let out = run_output(
        r#"struct Pair<A, B> { first: A, second: B }
fn main() {
    val p = Pair { first: 1, second: 99 }
    print(format("{}", p.second))
}"#,
    );
    assert!(out.contains("99"), "got: {out:?}");
}
