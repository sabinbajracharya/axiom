//! Builtin functions — VM-level implementations for extern "C" fns and
//! compiler-intrinsic methods.
//!
//! Single-file path: `print`/`println` are dispatched here as name-based
//! builtins (stdlib HIR not loaded).
//!
//! Multi-file path: `write_string`/`write_line` are dispatched here via the
//! `is_extern` flag on their IR function entries. `String::len` is dispatched
//! via the method-call builtin check.

use crate::error::VmError;
use crate::trace::ExecutionTrace;
use crate::value::Value;

/// Check if a function/method name is a builtin.
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "print" | "println" | "write_string" | "write_line" | "String::len"
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
        "write_string" => builtin_write(args, trace, false),
        "write_line" => builtin_write(args, trace, true),
        "String::len" => builtin_string_len(args),
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

/// `write_string` / `write_line` — platform I/O for the multi-file path.
fn builtin_write(
    args: Vec<Value>,
    trace: &mut Option<ExecutionTrace>,
    newline: bool,
) -> Result<Value, VmError> {
    let text = extract_string_arg(&args, 1)?;
    if newline {
        println!("{text}");
    } else {
        print!("{text}");
    }
    let label = if newline {
        "write_line"
    } else {
        "write_string"
    };
    if let Some(t) = trace {
        t.record("builtin", format!("{label}({text})"), Some(Value::Unit));
        let output = if newline {
            format!("{text}\n")
        } else {
            text.clone()
        };
        t.record("output", output, None);
    }
    Ok(Value::Unit)
}

/// `String::len` — returns the byte length of a string.
fn builtin_string_len(args: Vec<Value>) -> Result<Value, VmError> {
    let s = extract_string_arg(&args, 0)?;
    Ok(Value::Int(s.len() as i64))
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin() {
        assert!(is_builtin("print"));
        assert!(is_builtin("println"));
        assert!(is_builtin("write_string"));
        assert!(is_builtin("write_line"));
        assert!(is_builtin("String::len"));
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
    fn test_write_string_returns_unit() {
        let result = call_builtin(
            "write_string",
            vec![Value::Int(1), Value::String("hi".into())],
            &mut None,
        )
        .unwrap();
        assert_eq!(result, Value::Unit);
    }

    #[test]
    fn test_write_line_returns_unit() {
        let result = call_builtin(
            "write_line",
            vec![Value::Int(1), Value::String("hi".into())],
            &mut None,
        )
        .unwrap();
        assert_eq!(result, Value::Unit);
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
