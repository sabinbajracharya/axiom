//! End-to-end: a trait method with a *default body* (no per-type override) is
//! dispatched correctly — both on a concrete receiver and through a generic
//! bound — and a per-type override still wins.
//!
//! Assertions use *computed* values that never appear as literals in the source
//! so a match proves the default body actually ran (its `self.legs()` call
//! dispatched to the concrete impl), not a coincidental constant in the trace.

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

const PROG: &str = r#"trait Animal {
    fn legs(let self) -> Int;
    fn score(let self) -> Int { self.legs() * self.legs() }
}
struct Dog { age: Int }
struct Spider { web: Int }
impl Animal for Dog {
    fn legs(let self) -> Int { 4 }
}
impl Animal for Spider {
    fn legs(let self) -> Int { 8 }
    fn score(let self) -> Int { self.legs() * 10 + 1 }
}
fn via_bound<T: Animal>(let x: T) -> Int { x.score() }
fn main() {
    val d = Dog { age: 3 }
    val s = Spider { web: 1 }
    print(format("{}", d.score()))
    print(format("{}", s.score()))
    print(format("{}", via_bound(d)))
}"#;

#[test]
fn test_default_method_dispatches_on_concrete_receiver() {
    // Dog has no `score` override → the trait default runs, calling Dog::legs (4)
    // → 4 * 4 = 16. 16 appears nowhere as a literal in the source.
    let out = run_output(PROG);
    assert!(
        out.contains("16"),
        "expected default score 16, got: {out:?}"
    );
}

#[test]
fn test_default_method_override_wins() {
    // Spider overrides `score`: 8 * 10 + 1 = 81 — a value with no literal source.
    let out = run_output(PROG);
    assert!(
        out.contains("81"),
        "expected overridden score 81, got: {out:?}"
    );
}

#[test]
fn test_default_method_through_generic_bound() {
    // via_bound(d) calls x.score() through `T: Animal`; with no override the
    // default runs and dispatches self.legs() to Dog::legs → 16.
    let out = run_output(PROG);
    let count = out.matches("16").count();
    assert!(
        count >= 2,
        "expected 16 twice (concrete + via bound), got {count} in: {out:?}"
    );
}
