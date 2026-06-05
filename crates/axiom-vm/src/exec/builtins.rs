//! Builtin functions — VM-level implementations for extern "C" fns and
//! compiler-intrinsic methods.
//!
//! Single-file path: `print`/`println` are dispatched here as name-based
//! builtins (stdlib HIR not loaded).
//!
//! Multi-file path: `write` is dispatched here via the `is_extern` flag on
//! its IR function entry. `String::len` and `String::as_bytes` are dispatched
//! via the method-call builtin check.

use crate::error::VmError;
use crate::trace::ExecutionTrace;
use crate::value::Value;

/// Check if a function/method name is a builtin.
/// Accepts both bare names (single-file) and module-qualified names (multi-file).
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "print"
            | "println"
            | "write"
            | "core::platform::write"
            | "String::len"
            | "String::as_bytes"
    )
}

/// Call a builtin function.
pub fn call_builtin(
    name: &str,
    args: Vec<Value>,
    trace: &mut Option<ExecutionTrace>,
) -> Result<Value, VmError> {
    match name {
        "print" | "println" => builtin_print(name, args, trace),
        "write" | "core::platform::write" => builtin_write(args, trace),
        "String::len" => builtin_string_len(args),
        "String::as_bytes" => builtin_string_as_bytes(args),
        _ => Err(VmError::BuiltinNotFound {
            name: name.to_string(),
        }),
    }
}

/// Legacy `print`/`println` builtin — used by the single-file path.
fn builtin_print(
    name: &str,
    args: Vec<Value>,
    trace: &mut Option<ExecutionTrace>,
) -> Result<Value, VmError> {
    let text: String = args
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    if name == "println" {
        println!("{text}");
    } else {
        print!("{text}");
    }
    if let Some(t) = trace {
        let suffix = if name == "println" { "\n" } else { "" };
        t.record("builtin", format!("{name}({text})"), Some(Value::Unit));
        t.record("output", format!("{text}{suffix}"), None);
    }
    Ok(Value::Unit)
}

/// `write(fd, buf: &[U8])` — platform I/O primitive.
/// Writes raw bytes to stdout (fd 1) or stderr (fd 2).
fn builtin_write(args: Vec<Value>, trace: &mut Option<ExecutionTrace>) -> Result<Value, VmError> {
    let bytes = extract_bytes_arg(&args, 1)?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    print!("{text}");
    if let Some(t) = trace {
        t.record("builtin", format!("write({text})"), Some(Value::Unit));
        t.record("output", text, None);
    }
    Ok(Value::Unit)
}

/// `String::len` — returns the byte length of a string.
fn builtin_string_len(args: Vec<Value>) -> Result<Value, VmError> {
    let s = extract_string_arg(&args, 0)?;
    Ok(Value::Int(s.len() as i64))
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
        assert!(is_builtin("print"));
        assert!(is_builtin("println"));
        assert!(is_builtin("write"));
        assert!(is_builtin("String::len"));
        assert!(is_builtin("String::as_bytes"));
        assert!(!is_builtin("main"));
    }

    #[test]
    fn test_print_returns_unit() {
        let result = call_builtin("print", vec![Value::Int(42)], &mut None).unwrap();
        assert_eq!(result, Value::Unit);
    }

    #[test]
    fn test_print_multiple_args() {
        let result = call_builtin(
            "println",
            vec![Value::String("hello".into()), Value::Int(42)],
            &mut None,
        )
        .unwrap();
        assert_eq!(result, Value::Unit);
    }

    #[test]
    fn test_write_returns_unit() {
        let result = call_builtin(
            "write",
            vec![Value::Int(1), Value::Bytes(b"hi".to_vec())],
            &mut None,
        )
        .unwrap();
        assert_eq!(result, Value::Unit);
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
    fn test_string_len() {
        let result = call_builtin(
            "String::len",
            vec![Value::String("hello".into())],
            &mut None,
        )
        .unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_string_len_empty() {
        let result =
            call_builtin("String::len", vec![Value::String("".into())], &mut None).unwrap();
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
