//! Extern fn (`extern "C" fn …;`) type-checking.
//!
//! Extern fns declare a signature but have no body — the platform supplies the
//! implementation. The type checker must record the signature and skip the
//! body/return reconciliation it applies to ordinary functions. See
//! `docs/extern-buffers-and-path-unification.md`.

#![allow(clippy::unwrap_used)]

fn check_source_with_stdlib(src: &str) -> typecheck::Thir {
    driver::check_modules(&stdlib::with_main(src))
}

fn type_diags(src: &str) -> Vec<String> {
    check_source_with_stdlib(src)
        .diagnostics
        .iter()
        .filter_map(|d| {
            if let typecheck::Diagnostic::Type(td) = d {
                Some(format!("{td:?}"))
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn test_extern_fn_with_return_type_is_clean() {
    // A non-Unit return type on a bodiless extern fn must NOT trip the
    // return-type-vs-body check (the body is empty → would otherwise be Unit).
    let d = type_diags(r#"extern "C" fn now() -> Int;"#);
    assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
}

#[test]
fn test_extern_fn_bytes_buffer_param_accepts_as_bytes() {
    // The platform-boundary shape: a `Bytes` buffer passed by convention. A
    // call passing `String::as_bytes()` must type-check.
    let src = r#"
extern "C" fn mywrite(fd: Int, let buf: Bytes) -> Int;
fn main() { let n = mywrite(1, "hi".as_bytes()) }
"#;
    let d = type_diags(src);
    assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
}

#[test]
fn test_extern_fn_inout_buffer_param() {
    // `read`'s mutable-buffer shape: `inout buf: Bytes`.
    let src = r#"extern "C" fn myread(fd: Int, inout buf: Bytes) -> Int;"#;
    let d = type_diags(src);
    assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
}
