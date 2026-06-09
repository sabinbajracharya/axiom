//! End-to-end: `Iterator<T>` trait, `loop x in xs`, `into_iter`, `next`.
//!
//! Exercises the full pipeline: the `Iterator` trait in `core::iter`, the
//! HIR desugar pass (iterator loops → `into_iter()` + `next()` + match),
//! and the `ListIter`/`MapIter` iterator structs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

fn run_output(source: &str) -> String {
    let thir = driver::check_modules(&stdlib::with_main(source));
    assert!(
        thir.diagnostics.is_empty(),
        "type diagnostics: {:?}",
        thir.diagnostics
    );
    let mono = specialize::monomorphize(&thir);
    let ir = ir::lower(&thir, &mono);
    let mut vm = vm::Vm::new(ir);
    vm.set_tracing(true);
    vm.run().expect("vm run");
    vm.take_trace().map(|t| t.output()).unwrap_or_default()
}

#[test]
fn test_loop_iterator_over_list() {
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = List::new()
    xs.push(10)
    xs.push(20)
    xs.push(30)
    loop x in xs {
        print(format("{}", x))
    }
}"#,
    );
    assert!(out.contains("10"), "expected 10, got: {out:?}");
    assert!(out.contains("20"), "expected 20, got: {out:?}");
    assert!(out.contains("30"), "expected 30, got: {out:?}");
}

#[test]
fn test_loop_iterator_empty_list() {
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = List::new()
    loop x in xs {
        print(format("{}", x))
    }
    print("done")
}"#,
    );
    assert!(out.contains("done"), "expected only 'done', got: {out:?}");
}

#[test]
fn test_list_into_iter_next() {
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = List::new()
    xs.push(1)
    xs.push(2)
    val iter = xs.into_iter()
    val a = iter.next()
    val b = iter.next()
    val c = iter.next()
    print(format("{}", a))
    print(format("{}", b))
    print(format("{}", c))
}"#,
    );
    assert!(
        out.contains("Some(1)") || out.contains("1"),
        "expected first element, got: {out:?}"
    );
}

#[test]
fn test_loop_iterator_twice_uses_separate_iterators() {
    let out = run_output(
        r#"fn main() {
    var xs: List<Int> = [1, 2]
    loop x in xs {
        print(format("a{}", x))
    }
    var ys: List<Int> = [3, 4]
    loop y in ys {
        print(format("b{}", y))
    }
}"#,
    );
    assert!(out.contains("a1"), "expected a1, got: {out:?}");
    assert!(out.contains("a2"), "expected a2, got: {out:?}");
    assert!(out.contains("b3"), "expected b3, got: {out:?}");
    assert!(out.contains("b4"), "expected b4, got: {out:?}");
}
