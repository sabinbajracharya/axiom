//! Type-checking tests for the `HeapBuffer<T>` floor ops (P4) — the growable
//! storage primitive (`[T]`) underlying the `List`/`Map` library.
//!
//! Exercises the return-only type-parameter inference for `heap_alloc`: its
//! element type `T` is bound from the binding's declared type (no argument
//! constrains it). `heap_get`/`heap_set` constrain `T` through the buffer.

#![allow(clippy::unwrap_used)]

use axiom_typeck::Thir;

fn check_source(source: &str) -> Thir {
    axiom_typeck::check_modules(&axiom_stdlib::with_main(source))
}

fn has_type_error(thir: &Thir) -> bool {
    thir.diagnostics
        .iter()
        .any(|d| d.kind() == "type_mismatch" || d.kind() == "not_yet_supported")
}

#[test]
fn test_heap_alloc_return_only_param_bound_from_annotation() {
    // `heap_alloc(n)` has a return-only `T`; the `[Int]` annotation binds it.
    let thir = check_source(
        r#"fn main() {
    var buf: [Int] = heap_alloc(4)
    heap_free(buf)
}"#,
    );
    assert!(
        !has_type_error(&thir),
        "unexpected errors: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_heap_get_set_constrain_element_type() {
    let thir = check_source(
        r#"fn main() {
    var buf: [Int] = heap_alloc(2)
    heap_set(buf, 0, 7)
    val x = heap_get(buf, 0)
    print(format("{}", x))
    heap_free(buf)
}"#,
    );
    assert!(
        !has_type_error(&thir),
        "unexpected errors: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_heap_set_wrong_element_type_is_rejected() {
    // Buffer is `[Int]`; storing a `String` must be a type error (the negative
    // case that proves unification is actually checking, not rubber-stamping).
    let thir = check_source(
        r#"fn main() {
    var buf: [Int] = heap_alloc(1)
    heap_set(buf, 0, "nope")
    heap_free(buf)
}"#,
    );
    assert!(
        thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "expected a type mismatch, got: {:?}",
        thir.diagnostics
    );
}

#[test]
fn test_heap_alloc_annotation_mismatch_is_rejected() {
    // Annotating a non-buffer type for a `heap_alloc` result is a mismatch.
    let thir = check_source(
        r#"fn main() {
    val buf: Int = heap_alloc(1)
}"#,
    );
    assert!(
        thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "expected a type mismatch, got: {:?}",
        thir.diagnostics
    );
}
