//! End-to-end: `Map<K, V>` as real Axiom library code (M7) — an open-addressing
//! hash table on `HeapBuffer<T>`, with no compiler intrinsics left for `Map`.
//!
//! Exercises set/get/has/count, key overwrite, `None` for absent keys, hashing
//! keys through the `Hashable` bound (`key.hash()` dispatched by runtime type),
//! and growth/rehash across the load-factor boundary.

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
fn test_map_set_get_and_count() {
    let out = run_output(
        r#"fn main() {
    var m: Map<Int, Int> = Map::new()
    m.set(1, 100)
    m.set(2, 200)
    m.set(3, 300)
    print(format("{}", m.count()))
    val r = match m.get(2) {
        Some(v) => v,
        None => -1,
    }
    print(format("{}", r))
}"#,
    );
    assert!(out.contains('3'), "expected count 3, got: {out:?}");
    assert!(out.contains("200"), "expected value 200, got: {out:?}");
}

#[test]
fn test_map_get_absent_is_none() {
    let out = run_output(
        r#"fn main() {
    var m: Map<Int, Int> = Map::new()
    m.set(1, 10)
    val r = match m.get(99) {
        Some(v) => v,
        None => -7,
    }
    print(format("{}", r))
    print(format("{}", m.has(99)))
    print(format("{}", m.has(1)))
}"#,
    );
    assert!(
        out.contains("-7"),
        "expected None sentinel -7, got: {out:?}"
    );
    assert!(
        out.contains("false"),
        "expected has(99)=false, got: {out:?}"
    );
    assert!(out.contains("true"), "expected has(1)=true, got: {out:?}");
}

#[test]
fn test_map_overwrite_keeps_count() {
    let out = run_output(
        r#"fn main() {
    var m: Map<Int, Int> = Map::new()
    m.set(5, 1)
    m.set(5, 2)
    m.set(5, 3)
    print(format("{}", m.count()))
    val r = match m.get(5) {
        Some(v) => v,
        None => 0,
    }
    print(format("{}", r))
}"#,
    );
    assert!(out.contains('1'), "expected count 1, got: {out:?}");
    assert!(out.contains('3'), "expected latest value 3, got: {out:?}");
}

#[test]
fn test_map_grows_and_rehashes() {
    // Insert enough keys to cross the 0.75 load factor (cap 8 → 16 → …) and
    // confirm every key still resolves after rehashing.
    let out = run_output(
        r#"fn main() {
    var m: Map<Int, Int> = Map::new()
    var i = 0
    loop if i < 20 {
        m.set(i, i * 10)
        i = i + 1
    }
    print(format("{}", m.count()))
    val a = match m.get(0) { Some(v) => v, None => -1 }
    val b = match m.get(19) { Some(v) => v, None => -1 }
    print(format("{}", a))
    print(format("{}", b))
}"#,
    );
    assert!(out.contains("20"), "expected count 20, got: {out:?}");
    assert!(out.contains("190"), "expected m[19]=190, got: {out:?}");
}

#[test]
fn test_map_string_keys() {
    let out = run_output(
        r#"fn main() {
    var m: Map<String, Int> = Map::new()
    m.set("hi", 1)
    m.set("bye", 2)
    val r = match m.get("bye") {
        Some(v) => v,
        None => 0,
    }
    print(format("{}", r))
}"#,
    );
    assert!(
        out.contains('2'),
        "expected value 2 for \"bye\", got: {out:?}"
    );
}
