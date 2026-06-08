//! End-to-end: assignment to *places* — struct fields (`c.n = 9`) and indexed
//! elements (`xs[0] = 9`). Previously `resolve_assign_target` returned a
//! sentinel and these lowered to nothing. They are the foundation for the
//! mutating collection methods (List::push writes `self.buf[i]` and
//! `self.count`).
//!
//! Assertions use `trace.output()` (the program's *real* printed text), so a
//! silent no-op cannot pass on a coincidental trace substring — the gap that
//! hid the indexed-write bug (`docs/mutable-subscript-design.md` §6).

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
fn test_struct_field_assignment_runs() {
    let out = run_output(
        r#"struct Counter { n: Int }
fn main() {
    var c = Counter { n: 5 }
    c.n = 9
    print(format("{}", c.n))
}"#,
    );
    assert_eq!(out, "9", "got: {out:?}");
}

#[test]
fn test_struct_field_compound_assignment_runs() {
    let out = run_output(
        r#"struct Counter { n: Int }
fn main() {
    var c = Counter { n: 5 }
    c.n = c.n + 3
    print(format("{}", c.n))
}"#,
    );
    assert_eq!(out, "8", "got: {out:?}");
}

#[test]
fn test_list_index_assignment_runs() {
    let out = run_output(
        r#"fn main() {
    var xs = [1, 2, 3]
    xs[0] = 9
    print(format("{}", xs[0]))
}"#,
    );
    assert_eq!(out, "9", "got: {out:?}");
}

#[test]
fn test_multi_index_subscript() {
    // A struct with a 2-index subscript; g[1,2] = 99 then read back.
    let out = run_output(
        r#"use std::mem::alloc_buffer
struct Grid { buf: [Int], cols: Int }
impl Grid {
    subscript(self, row: Int, col: Int) -> Int {
        self.buf[row * self.cols + col]
    }
    subscript(inout self, row: Int, col: Int, value: Int) {
        self.buf[row * self.cols + col] = value
    }
}
fn main() {
    var buf: [Int] = alloc_buffer(6)
    buf[0] = 1
    buf[1] = 2
    buf[2] = 3
    buf[3] = 4
    buf[4] = 5
    buf[5] = 6
    var g = Grid { buf: buf, cols: 3 }
    g[1, 2] = 99
    print(format("{}", g[1, 2]))
}"#,
    );
    assert_eq!(out, "99", "got: {out:?}");
}

#[test]
fn test_multi_index_compound() {
    // Compound op on a multi-index subscript: g[0,1] += 10 → 2 + 10 = 12.
    let out = run_output(
        r#"use std::mem::alloc_buffer
struct Grid { buf: [Int], cols: Int }
impl Grid {
    subscript(self, row: Int, col: Int) -> Int {
        self.buf[row * self.cols + col]
    }
    subscript(inout self, row: Int, col: Int, value: Int) {
        self.buf[row * self.cols + col] = value
    }
}
fn main() {
    var buf: [Int] = alloc_buffer(6)
    buf[0] = 1
    buf[1] = 2
    buf[2] = 3
    buf[3] = 4
    buf[4] = 5
    buf[5] = 6
    var g = Grid { buf: buf, cols: 3 }
    g[0, 1] += 10
    print(format("{}", g[0, 1]))
}"#,
    );
    assert_eq!(out, "12", "got: {out:?}");
}
