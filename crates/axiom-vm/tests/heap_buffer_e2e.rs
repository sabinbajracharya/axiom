//! End-to-end: the `HeapBuffer<T>` growable-storage primitive (`[T]`) is usable
//! from Axiom code via the four intrinsic ops — `heap_alloc`, `heap_set`,
//! `heap_get`, `heap_free`. This is P4: the floor that the real `List<T>` and
//! `Map<K,V>` library implementations are built on (Phase D of the
//! builtin-to-stdlib migration).
//!
//! Key type-system exercise: `heap_alloc(n)` has a *return-only* type parameter
//! `T` (no argument constrains it), so its element type is bound from the
//! binding's declared type via bidirectional inference. `heap_get`/`heap_set`
//! constrain `T` through the buffer argument.

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
fn test_heap_buffer_set_get_roundtrip() {
    let out = run_output(
        r#"fn main() {
    var buf: [Int] = heap_alloc(3)
    heap_set(buf, 0, 10)
    heap_set(buf, 1, 20)
    heap_set(buf, 2, 30)
    val a = heap_get(buf, 1)
    print(format("{}", a))
    heap_free(buf)
}"#,
    );
    assert!(out.contains("20"), "got: {out:?}");
}

#[test]
fn test_heap_buffer_index_read() {
    // The buffer is a `HeapPtr`, so `Index` reads (`buf[i]`) work directly.
    let out = run_output(
        r#"fn main() {
    var buf: [Int] = heap_alloc(2)
    heap_set(buf, 0, 7)
    heap_set(buf, 1, 99)
    print(format("{}", buf[1]))
    heap_free(buf)
}"#,
    );
    assert!(out.contains("99"), "got: {out:?}");
}

#[test]
fn test_heap_buffer_index_set() {
    // `buf[i] = v` lowers to `IndexSet` on the `HeapPtr`.
    let out = run_output(
        r#"fn main() {
    var buf: [Int] = heap_alloc(2)
    buf[0] = 41
    buf[1] = 42
    print(format("{}", buf[1]))
    heap_free(buf)
}"#,
    );
    assert!(out.contains("42"), "got: {out:?}");
}
