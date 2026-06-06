//! End-to-end: `List<T>` as real Axiom library code on `HeapBuffer<T>` (M6).
//!
//! Exercises the whole stack — generic struct with a `[T]` field, an
//! associated constructor (`List::new`), `inout self` mutators (`push`/`grow`),
//! buffer growth, field/index assignment, and subscript reads — with no
//! compiler intrinsics left for `List`.

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
fn test_list_push_count_and_subscript() {
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = List::new()
    xs.push(10)
    xs.push(20)
    xs.push(30)
    print(format("{}", xs.count()))
    print(format("{}", xs[1]))
}"#,
    );
    assert!(out.contains('3'), "expected count 3, got: {out:?}");
    assert!(out.contains("20"), "expected element 20, got: {out:?}");
}

#[test]
fn test_list_literal_desugars_to_stdlib() {
    // `[10, 20, 30]` is sugar for `List::new()` + a `push` per element — it must
    // produce a real `List<Int>` struct, so `count()`/subscript work on it just
    // like on a list built by hand. (Before M6's literal migration this lowered
    // to a compiler-native `Value::List` and silently returned `()`.)
    // Distinctive prefixes so the assertions match the *rendered* output, not a
    // stray element literal echoed in the trace.
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = [10, 20, 30]
    print(format("count={}", xs.count()))
    print(format("elem={}", xs[1]))
}"#,
    );
    assert!(out.contains("count=3"), "expected count 3, got: {out:?}");
    assert!(out.contains("elem=20"), "expected element 20, got: {out:?}");
}

#[test]
fn test_list_literal_preallocates_exact_capacity() {
    // A fixed-size literal must allocate exactly once: `[10, 20, 30]` lowers to
    // `List::with_capacity(3)` + pushes, so capacity is exactly 3 — not the 4
    // that `new()` + grow-doubling (0 → 4) would leave.
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = [10, 20, 30]
    print(format("cap={}", xs.capacity()))
}"#,
    );
    assert!(out.contains("cap=3"), "expected capacity 3, got: {out:?}");
}

#[test]
fn test_list_grows_past_initial_capacity() {
    // Initial cap is 0 → grows to 4 → grows to 8; pushing 5 elements crosses a
    // growth boundary and must preserve earlier elements.
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = List::new()
    xs.push(1)
    xs.push(2)
    xs.push(3)
    xs.push(4)
    xs.push(5)
    print(format("{}", xs.count()))
    print(format("{}", xs[0]))
    print(format("{}", xs[4]))
}"#,
    );
    assert!(out.contains('5'), "expected count 5, got: {out:?}");
    assert!(out.contains('1'), "expected first element 1, got: {out:?}");
}

#[test]
fn test_list_is_empty() {
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = List::new()
    print(format("{}", xs.is_empty()))
    xs.push(7)
    print(format("{}", xs.is_empty()))
}"#,
    );
    assert!(out.contains("true"), "expected true (empty), got: {out:?}");
    assert!(
        out.contains("false"),
        "expected false (non-empty), got: {out:?}"
    );
}
