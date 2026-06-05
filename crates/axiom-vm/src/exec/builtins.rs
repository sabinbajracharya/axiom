//! Builtin functions (print, println).

use crate::error::VmError;
use crate::trace::ExecutionTrace;
use crate::value::Value;

/// Check if a function name is a builtin.
pub fn is_builtin(name: &str) -> bool {
    matches!(name, "print" | "println")
}

/// Call a builtin function.
pub fn call_builtin(
    name: &str,
    args: Vec<Value>,
    trace: &mut Option<ExecutionTrace>,
) -> Result<Value, VmError> {
    match name {
        "print" | "println" => {
            let text: String = args
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            // Record in trace if active.
            if let Some(t) = trace {
                let suffix = if name == "println" { "\n" } else { "" };
                t.record("builtin", format!("{name}({text})"), Some(Value::Unit));
                // Also capture the output for golden tests.
                t.record("output", format!("{text}{suffix}"), None);
            }
            // For now, we don't actually print to stdout in the VM —
            // output goes through the trace. Tests verify the trace.
            Ok(Value::Unit)
        }
        _ => Err(VmError::BuiltinNotFound {
            name: name.to_string(),
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
    fn test_unknown_builtin() {
        assert!(matches!(
            call_builtin("unknown", vec![], &mut None),
            Err(VmError::BuiltinNotFound { .. })
        ));
    }
}
