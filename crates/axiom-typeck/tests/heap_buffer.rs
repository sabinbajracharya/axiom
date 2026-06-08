//! Type-checking tests for the `HeapBuffer<T>` floor ops (P4) — the growable
//! storage primitive (`[T]`) underlying the `List`/`Map` library.
//!
//! Exercises the return-only type-parameter inference for `alloc_buffer`: its
//! element type `T` is bound from the binding's declared type (no argument
//! constrains it). `get_buffer`/`set_buffer` constrain `T` through the buffer.
//!
//! These intrinsics live in `std::mem` and must be imported explicitly —
//! they are no longer global names.

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
    let thir = check_source(
        r#"use std::mem::{alloc_buffer, free_buffer}
fn main() {
    var buf: [Int] = alloc_buffer(4)
    free_buffer(buf)
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
        r#"use std::mem::{alloc_buffer, free_buffer, get_buffer, set_buffer}
fn main() {
    var buf: [Int] = alloc_buffer(2)
    set_buffer(buf, 0, 7)
    val x = get_buffer(buf, 0)
    print(format("{}", x))
    free_buffer(buf)
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
    let thir = check_source(
        r#"use std::mem::{alloc_buffer, free_buffer, set_buffer}
fn main() {
    var buf: [Int] = alloc_buffer(1)
    set_buffer(buf, 0, "nope")
    free_buffer(buf)
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
    let thir = check_source(
        r#"use std::mem::alloc_buffer
fn main() {
    val buf: Int = alloc_buffer(1)
}"#,
    );
    assert!(
        thir.diagnostics.iter().any(|d| d.kind() == "type_mismatch"),
        "expected a type mismatch, got: {:?}",
        thir.diagnostics
    );
}
