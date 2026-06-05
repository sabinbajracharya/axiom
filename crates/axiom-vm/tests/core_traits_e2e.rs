//! End-to-end: the core trait impls for primitives (core/primitives.ax,
//! core/string.ax) type-check, dispatch, and run on the VM.

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
fn test_int_equatable_dispatch_runs() {
    let out = run_output(
        r#"fn main() { if (2).eq(2) { print("eq-ok") } if (1).eq(2) { print("bad") } else { print("ne-ok") } }"#,
    );
    assert!(out.contains("eq-ok"), "got: {out:?}");
    assert!(out.contains("ne-ok"), "got: {out:?}");
}

#[test]
fn test_int_ord_dispatch_runs() {
    let out = run_output(
        r#"fn main() { if (1).lt(2) { print("lt-ok") } if (2).lt(1) { print("bad") } else { print("ge-ok") } }"#,
    );
    assert!(out.contains("lt-ok"), "got: {out:?}");
    assert!(out.contains("ge-ok"), "got: {out:?}");
}

#[test]
fn test_string_ord_dispatch_runs() {
    let out = run_output(r#"fn main() { if ("apple").lt("banana") { print("str-lt-ok") } }"#);
    assert!(out.contains("str-lt-ok"), "got: {out:?}");
}

#[test]
fn test_hashable_dispatch_runs() {
    // hash is deterministic and equal values hash equal: Int identity, String FNV-1a.
    let out = run_output(
        r#"fn main() {
    if (7).hash().eq(7) { print("int-hash-ok") }
    if ("x").hash().eq(("x").hash()) { print("str-hash-stable") }
}"#,
    );
    assert!(out.contains("int-hash-ok"), "got: {out:?}");
    assert!(out.contains("str-hash-stable"), "got: {out:?}");
}

#[test]
fn test_hashable_bound_satisfied() {
    // T: Hashable is satisfied for primitives via the core impls (and its
    // Equatable supertrait resolves through them too).
    let out = run_output(
        r#"fn need_hash<T: Hashable>(let x: T) {}
fn main() { need_hash(1) need_hash("k") print("bound-ok") }"#,
    );
    assert!(out.contains("bound-ok"), "got: {out:?}");
}
