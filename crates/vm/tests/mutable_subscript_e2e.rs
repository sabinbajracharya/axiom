//! End-to-end: indexed-place *writes* — `base[i] = v` and `base[i] op= v` — on
//! library collections (`List<T>`) and user structs with a write subscript.
//!
//! These assert on the program's **real output** (`trace.output()`, the `output`
//! entries only — `docs/mutable-subscript-design.md` §7 H1), never on a substring
//! of the full execution trace. Every asserted value is **computed at runtime**
//! and never appears verbatim as a source literal (§7 H2), so a silent no-op
//! cannot pass and a value cannot leak in from a `Const`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

/// Run `source` and return only what the program actually printed.
fn run_program(source: &str) -> String {
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
fn test_list_index_plain_assignment_writes_element() {
    // xs[0] becomes 45 (40 + 5); 45 appears nowhere as a literal.
    let out = run_program(
        r#"fn main() {
    var xs = [1, 2, 3]
    xs[0] = 40 + 5
    print(format("{}", xs[0]))
}"#,
    );
    assert_eq!(out, "45", "xs[0] should be the written 45, got: {out:?}");
}

#[test]
fn test_list_index_assignment_from_other_element() {
    // x[0] = x[1] + x[2] = 20 + 30 = 50; 50 is not a literal.
    let out = run_program(
        r#"fn main() {
    var x = [10, 20, 30]
    x[0] = x[1] + x[2]
    print(format("{}", x[0]))
}"#,
    );
    assert_eq!(out, "50", "x[0] should be 50, got: {out:?}");
}

#[test]
fn test_list_index_compound_add_assignment() {
    // a[1] += 40 → 2 + 40 = 42; 42 is not a literal.
    let out = run_program(
        r#"fn main() {
    var a = [1, 2, 3]
    a[1] += 40
    print(format("{}", a[1]))
}"#,
    );
    assert_eq!(out, "42", "a[1] should be 42, got: {out:?}");
}

#[test]
fn test_list_index_compound_sub_assignment() {
    // a[1] -= 5 → 20 - 5 = 15; 15 is not a literal.
    let out = run_program(
        r#"fn main() {
    var a = [10, 20, 30]
    a[1] -= 5
    print(format("{}", a[1]))
}"#,
    );
    assert_eq!(out, "15", "a[1] should be 15, got: {out:?}");
}

#[test]
fn test_list_index_write_then_read_other_element_untouched() {
    // Writing xs[0] must not disturb xs[2]; print both: 45 then 3 → "453".
    let out = run_program(
        r#"fn main() {
    var xs = [1, 2, 3]
    xs[0] = 40 + 5
    print(format("{}", xs[0]))
    print(format("{}", xs[2]))
}"#,
    );
    assert_eq!(out, "453", "expected xs[0]=45, xs[2]=3, got: {out:?}");
}

#[test]
fn test_user_struct_write_subscript_plain() {
    // A user struct with read + write subscripts; p[0] = 47 then read back.
    let out = run_program(
        r#"struct Pair { a: Int, b: Int }
impl Pair {
    subscript(self, i: Int) -> Int {
        if i == 0 { self.a } else { self.b }
    }
    subscript(inout self, i: Int, value: Int) {
        if i == 0 { self.a = value } else { self.b = value }
    }
}
fn main() {
    var p = Pair { a: 1, b: 2 }
    p[0] = 40 + 7
    print(format("{}", p[0]))
}"#,
    );
    assert_eq!(out, "47", "p[0] should be 47, got: {out:?}");
}

#[test]
fn test_user_struct_write_subscript_compound() {
    // Compound op on a user struct write subscript: p[1] += 40 → 2 + 40 = 42.
    let out = run_program(
        r#"struct Pair { a: Int, b: Int }
impl Pair {
    subscript(self, i: Int) -> Int {
        if i == 0 { self.a } else { self.b }
    }
    subscript(inout self, i: Int, value: Int) {
        if i == 0 { self.a = value } else { self.b = value }
    }
}
fn main() {
    var p = Pair { a: 1, b: 2 }
    p[1] += 40
    print(format("{}", p[1]))
}"#,
    );
    assert_eq!(out, "42", "p[1] should be 42, got: {out:?}");
}
