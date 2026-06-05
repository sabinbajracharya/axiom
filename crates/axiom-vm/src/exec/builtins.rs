//! Builtin functions — VM-level implementations for compiler-intrinsic methods
//! and the platform extern boundary.
//!
//! Two distinct mechanisms, kept separate:
//! - **Platform externs** (`core::platform::write`/`read`/`close`) are dispatched
//!   off the `IrFunction.is_extern` flag through the closed [`PlatformFn`] enum —
//!   never by ad-hoc name matching. The Rust bodies here stand in until real FFI
//!   (`dlsym`) lands with the native backend.
//! - **Method intrinsics** (`String::as_bytes`, `Bytes::len`, `<Prim>::hash_raw`)
//!   are dispatched via the method-call builtin check. `String::len` is *not*
//!   here — it is library code (`core/string.ax`) calling `as_bytes().len()`.
//! - **`format`** is the one variadic formatting intrinsic (DESIGN_SPEC §11),
//!   dispatched as a free-function builtin; it renders its template via the
//!   `Value` Display impl.
//!
//! `print`/`println` are NOT builtins — they are real Axiom functions in
//! `stdlib/std/io.ax` that call `core::platform::write`.

use crate::error::VmError;
use crate::trace::ExecutionTrace;
use crate::value::Value;

/// Check if a method name is a compiler intrinsic dispatched in the VM.
/// (Platform externs are dispatched via `is_extern`, not this check.)
///
/// `format` is the one variadic formatting intrinsic (the formatting primitive,
/// DESIGN_SPEC §11). A `string::format(...)` call lowers to a bare `format` call
/// (the lowerer keeps only the last path segment), so the VM matches `"format"`.
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "Bytes::len"
            | "String::as_bytes"
            | "format"
            | "Int::hash_raw"
            | "Float::hash_raw"
            | "Bool::hash_raw"
            | "String::hash_raw"
    )
}

/// Call a method intrinsic.
pub fn call_builtin(
    name: &str,
    args: Vec<Value>,
    _trace: &mut Option<ExecutionTrace>,
) -> Result<Value, VmError> {
    match name {
        "Bytes::len" => builtin_bytes_len(args),
        "String::as_bytes" => builtin_string_as_bytes(args),
        "format" => builtin_format(args),
        "Int::hash_raw" | "Float::hash_raw" | "Bool::hash_raw" | "String::hash_raw" => {
            builtin_hash_raw(args)
        }
        _ => Err(VmError::BuiltinNotFound {
            name: name.to_string(),
        }),
    }
}

/// The scalar `hash` floor primitive: a deterministic hash of a primitive value
/// to an `Int`. Deterministic (no per-process seed) so execution traces are
/// reproducible. Int hashes to itself, Bool to 0/1, Float to its bit pattern,
/// String via FNV-1a over its UTF-8 bytes. Equal values hash equal, satisfying
/// the `Hashable: Equatable` contract.
fn builtin_hash_raw(args: Vec<Value>) -> Result<Value, VmError> {
    let h = match args.first() {
        Some(Value::Int(n)) => *n,
        Some(Value::Bool(b)) => i64::from(*b),
        Some(Value::Float(f)) => f.to_bits() as i64,
        Some(Value::String(s)) => fnv1a(s.as_bytes()),
        other => {
            return Err(VmError::TypeError {
                expected: "Int|Float|Bool|String".to_string(),
                got: other
                    .map(|v| v.type_name())
                    .unwrap_or("missing")
                    .to_string(),
            })
        }
    };
    Ok(Value::Int(h))
}

/// FNV-1a 64-bit, returned as i64. A small, stable, dependency-free byte hash.
fn fnv1a(bytes: &[u8]) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash as i64
}

/// `format(template, args...)` — the runtime side of the `format` intrinsic.
/// The first argument is the template string; each subsequent argument fills
/// one `{}` (Display) or `{:?}` (debug) placeholder, in order. `{{` and `}}`
/// are literal-brace escapes. Built-in scalars render via the `Value` Display
/// impl. Returns a `Value::String`.
fn builtin_format(args: Vec<Value>) -> Result<Value, VmError> {
    let template = match args.first() {
        Some(Value::String(s)) => s.clone(),
        other => {
            return Err(VmError::TypeError {
                expected: "String".to_string(),
                got: other
                    .map(|v| v.type_name())
                    .unwrap_or("missing")
                    .to_string(),
            })
        }
    };
    Ok(Value::String(format_template(&template, &args[1..])))
}

/// Render a `format` template, consuming `args` in order. Mirrors the proven
/// Oxy template engine: `{}`/`{:?}` placeholders, `{{`/`}}` escapes.
fn format_template(template: &str, args: &[Value]) -> String {
    let mut out = String::new();
    let mut arg_idx = 0;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                // Consume up to the closing `}`, noting `?` for debug form.
                let mut is_debug = false;
                for cc in chars.by_ref() {
                    if cc == '}' {
                        break;
                    }
                    if cc == '?' {
                        is_debug = true;
                    }
                }
                if let Some(v) = args.get(arg_idx) {
                    out.push_str(&render_value(v, is_debug));
                }
                arg_idx += 1;
            }
            _ => out.push(c),
        }
    }
    out
}

/// Render a single value for a placeholder. Debug form quotes strings; both
/// forms otherwise use the `Value` Display impl.
fn render_value(v: &Value, is_debug: bool) -> String {
    match (is_debug, v) {
        (true, Value::String(s)) => format!("{s:?}"),
        _ => v.to_string(),
    }
}

/// The closed set of `core::platform` extern fns. Dispatched off the
/// `IrFunction.is_extern` flag via [`resolve_extern`] — the single place platform
/// fns are named. Exhaustive `match` (no wildcard) guards against silent drift,
/// mirroring the dual-backend divergence guards (DESIGN_SPEC §13.2).
pub enum PlatformFn {
    Write,
    Read,
    Close,
}

/// Map an extern fn's IR name to its [`PlatformFn`]. Accepts both bare
/// (single-file) and module-qualified (`core::platform::write`) forms by
/// matching the final path segment.
pub fn resolve_extern(name: &str) -> Option<PlatformFn> {
    match name.rsplit("::").next().unwrap_or(name) {
        "write" => Some(PlatformFn::Write),
        "read" => Some(PlatformFn::Read),
        "close" => Some(PlatformFn::Close),
        _ => None,
    }
}

/// Invoke a platform extern. (Real FFI replaces these bodies with the native backend.)
pub fn call_extern(
    f: PlatformFn,
    args: Vec<Value>,
    trace: &mut Option<ExecutionTrace>,
) -> Result<Value, VmError> {
    match f {
        PlatformFn::Write => builtin_write(args, trace),
        // No-op for the standard streams the VM supports; reports success.
        PlatformFn::Close => Ok(Value::Int(0)),
        // The tree-walking VM has no stdin; real input waits for the native backend.
        PlatformFn::Read => Err(VmError::ExternNotImplemented {
            name: "read".to_string(),
        }),
    }
}

/// `write(fd, buf: Bytes)` — platform I/O primitive.
/// Writes raw bytes to stdout (fd 1) or stderr (fd 2).
fn builtin_write(args: Vec<Value>, trace: &mut Option<ExecutionTrace>) -> Result<Value, VmError> {
    let bytes = extract_bytes_arg(&args, 1)?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    print!("{text}");
    if let Some(t) = trace {
        t.record("extern", format!("write({text})"), Some(Value::Unit));
        t.record("output", text, None);
    }
    Ok(Value::Unit)
}

/// `Bytes::len` — returns the length of a byte buffer. The irreducible length
/// floor; `String::len` is library code (core/string.ax) that calls
/// `self.as_bytes().len()`.
fn builtin_bytes_len(args: Vec<Value>) -> Result<Value, VmError> {
    match args.first() {
        Some(Value::Bytes(b)) => Ok(Value::Int(b.len() as i64)),
        other => Err(VmError::TypeError {
            expected: "Bytes".to_string(),
            got: other
                .map(|v| v.type_name())
                .unwrap_or("missing")
                .to_string(),
        }),
    }
}

/// `String::as_bytes` — returns the raw bytes of a string.
fn builtin_string_as_bytes(args: Vec<Value>) -> Result<Value, VmError> {
    let s = extract_string_arg(&args, 0)?;
    Ok(Value::Bytes(s.into_bytes()))
}

/// Extract a `Value::String` from `args[index]`.
fn extract_string_arg(args: &[Value], index: usize) -> Result<String, VmError> {
    match args.get(index) {
        Some(Value::String(s)) => Ok(s.clone()),
        other => Err(VmError::TypeError {
            expected: "String".to_string(),
            got: other
                .map(|v| v.type_name())
                .unwrap_or("missing")
                .to_string(),
        }),
    }
}

/// Extract a `Value::Bytes` from `args[index]`.
fn extract_bytes_arg(args: &[Value], index: usize) -> Result<Vec<u8>, VmError> {
    match args.get(index) {
        Some(Value::Bytes(b)) => Ok(b.clone()),
        other => Err(VmError::TypeError {
            expected: "Bytes".to_string(),
            got: other
                .map(|v| v.type_name())
                .unwrap_or("missing")
                .to_string(),
        }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin() {
        // Method intrinsics + the `format` free-function intrinsic.
        // print/println/write are NOT builtins.
        assert!(is_builtin("Bytes::len"));
        assert!(is_builtin("String::as_bytes"));
        assert!(is_builtin("format"));
        assert!(is_builtin("Int::hash_raw"));
        assert!(!is_builtin("String::len"));
        assert!(!is_builtin("print"));
        assert!(!is_builtin("println"));
        assert!(!is_builtin("write"));
        assert!(!is_builtin("main"));
    }

    #[test]
    fn test_format_renders_scalars() {
        let v = call_builtin(
            "format",
            vec![
                Value::String("{} = {}".into()),
                Value::String("x".into()),
                Value::Int(42),
            ],
            &mut None,
        )
        .unwrap();
        assert_eq!(v, Value::String("x = 42".into()));
    }

    #[test]
    fn test_format_float_bool() {
        let v = call_builtin(
            "format",
            vec![
                Value::String("{} {}".into()),
                Value::Float(3.5),
                Value::Bool(true),
            ],
            &mut None,
        )
        .unwrap();
        assert_eq!(v, Value::String("3.5 true".into()));
    }

    #[test]
    fn test_format_brace_escapes_and_debug() {
        let v = call_builtin(
            "format",
            vec![
                Value::String("{{{}}} {:?}".into()),
                Value::Int(1),
                Value::String("hi".into()),
            ],
            &mut None,
        )
        .unwrap();
        // `{{` and `}}` are literal braces; `{}` -> 1; `{:?}` quotes the string.
        assert_eq!(v, Value::String("{1} \"hi\"".into()));
    }

    #[test]
    fn test_format_no_template_args_leaves_placeholder_empty() {
        // Missing arg for a placeholder renders as nothing (lenient, like Oxy).
        let v = call_builtin("format", vec![Value::String("a{}b".into())], &mut None).unwrap();
        assert_eq!(v, Value::String("ab".into()));
    }

    #[test]
    fn test_format_non_string_template_errors() {
        assert!(matches!(
            call_builtin("format", vec![Value::Int(1)], &mut None),
            Err(VmError::TypeError { .. })
        ));
    }

    #[test]
    fn test_resolve_extern_bare_and_qualified() {
        assert!(matches!(resolve_extern("write"), Some(PlatformFn::Write)));
        assert!(matches!(
            resolve_extern("core::platform::write"),
            Some(PlatformFn::Write)
        ));
        assert!(matches!(resolve_extern("read"), Some(PlatformFn::Read)));
        assert!(matches!(resolve_extern("close"), Some(PlatformFn::Close)));
        assert!(resolve_extern("println").is_none());
    }

    #[test]
    fn test_call_extern_write_returns_unit() {
        let result = call_extern(
            PlatformFn::Write,
            vec![Value::Int(1), Value::Bytes(b"hi".to_vec())],
            &mut None,
        )
        .unwrap();
        assert_eq!(result, Value::Unit);
    }

    #[test]
    fn test_call_extern_close_succeeds() {
        let result = call_extern(PlatformFn::Close, vec![Value::Int(1)], &mut None).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_call_extern_read_not_implemented() {
        assert!(matches!(
            call_extern(PlatformFn::Read, vec![], &mut None),
            Err(VmError::ExternNotImplemented { .. })
        ));
    }

    #[test]
    fn test_string_as_bytes() {
        let result = call_builtin(
            "String::as_bytes",
            vec![Value::String("hi".into())],
            &mut None,
        )
        .unwrap();
        assert_eq!(result, Value::Bytes(vec![104, 105]));
    }

    #[test]
    fn test_bytes_len() {
        let result = call_builtin(
            "Bytes::len",
            vec![Value::Bytes(vec![104, 101, 108])],
            &mut None,
        )
        .unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_bytes_len_empty() {
        let result = call_builtin("Bytes::len", vec![Value::Bytes(vec![])], &mut None).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_unknown_builtin() {
        assert!(matches!(
            call_builtin("unknown", vec![], &mut None),
            Err(VmError::BuiltinNotFound { .. })
        ));
    }
}
