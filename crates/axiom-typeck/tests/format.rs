//! `string::format` — the one variadic formatting intrinsic (DESIGN_SPEC §11).
//! It accepts any number of arguments of any type and yields `String`; a
//! `string::format(...)` call lowers to a bare `format` call. See
//! `docs/string-format-and-print-retire.md`.

#![allow(clippy::unwrap_used)]

use axiom_typeck::{check_source_with_stdlib, serialize};

fn diags(src: &str) -> Vec<String> {
    check_source_with_stdlib(src)
        .diagnostics
        .iter()
        .map(|d| format!("{d:?}"))
        .collect()
}

#[test]
fn test_format_qualified_returns_string() {
    // `string::format(...)` type-checks and its result is a String, so binding
    // it with an explicit `: String` annotation is clean.
    let d = diags(r#"fn main() { val s: String = string::format("{}", 42) }"#);
    assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
}

#[test]
fn test_format_bare_call_also_works() {
    let d = diags(r#"fn main() { val s: String = format("{}", 42) }"#);
    assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
}

#[test]
fn test_format_is_variadic_any_arity() {
    // Zero, one, and many args all type-check (no arity check on the intrinsic).
    for src in [
        r#"fn main() { val s: String = format("none") }"#,
        r#"fn main() { val s: String = format("{}", 1) }"#,
        r#"fn main() { val s: String = format("{} {} {}", 1, true, "x") }"#,
    ] {
        let d = diags(src);
        assert!(d.is_empty(), "unexpected diagnostics for {src:?}: {d:?}");
    }
}

#[test]
fn test_format_accepts_mixed_arg_types() {
    let d = diags(r#"fn main() { val s: String = format("{} {} {}", 3.14, false, 7) }"#);
    assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
}

#[test]
fn test_format_result_feeds_string_only_print() {
    // The whole point: print is String-only, and format bridges a non-string.
    let d = diags(r#"fn main() { print(string::format("answer = {}", 42)) }"#);
    assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
}

#[test]
fn test_format_result_is_string_in_thir() {
    let thir = check_source_with_stdlib(r#"fn main() { val s = format("{}", 1) }"#);
    let dump = serialize(&thir, None);
    // The `val s` binding is inferred String from the format call.
    assert!(
        dump.contains("String"),
        "expected String in THIR dump:\n{dump}"
    );
}
