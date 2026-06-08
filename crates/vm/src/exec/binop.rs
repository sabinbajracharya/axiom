//! Binary and unary operation execution.

use resolver::{BinOp, UnaryOp};

use crate::error::VmError;
use crate::value::Value;

/// Evaluate a binary operation on two values.
pub fn exec_binop(op: BinOp, lhs: &Value, rhs: &Value) -> Result<Value, VmError> {
    match op {
        BinOp::Add => exec_add(lhs, rhs),
        BinOp::Sub => exec_arith(lhs, rhs, |a, b| a - b, |a, b| a - b),
        BinOp::Mul => exec_arith(lhs, rhs, |a, b| a * b, |a, b| a * b),
        BinOp::Div => exec_div(lhs, rhs),
        BinOp::Mod => exec_mod(lhs, rhs),
        BinOp::Eq => Ok(Value::Bool(lhs == rhs)),
        BinOp::Ne => Ok(Value::Bool(lhs != rhs)),
        BinOp::Lt => exec_cmp(lhs, rhs, |o| o == std::cmp::Ordering::Less),
        BinOp::Le => exec_cmp(lhs, rhs, |o| o != std::cmp::Ordering::Greater),
        BinOp::Gt => exec_cmp(lhs, rhs, |o| o == std::cmp::Ordering::Greater),
        BinOp::Ge => exec_cmp(lhs, rhs, |o| o != std::cmp::Ordering::Less),
        BinOp::And => exec_logical_and(lhs, rhs),
        BinOp::Or => exec_logical_or(lhs, rhs),
        BinOp::Shl => exec_bitwise(lhs, rhs, |a, b| a << b),
        BinOp::Shr => exec_bitwise(lhs, rhs, |a, b| a >> b),
        BinOp::BitAnd => exec_bitwise(lhs, rhs, |a, b| a & b),
        BinOp::BitOr => exec_bitwise(lhs, rhs, |a, b| a | b),
        BinOp::BitXor => exec_bitwise(lhs, rhs, |a, b| a ^ b),
    }
}

/// Evaluate a unary operation.
pub fn exec_unaryop(op: UnaryOp, src: &Value) -> Result<Value, VmError> {
    match op {
        UnaryOp::Neg => match src {
            Value::Int(n) => Ok(Value::Int(-n)),
            Value::Float(f) => Ok(Value::Float(-f)),
            _ => Err(VmError::BranchTypeMismatch {
                got: src.type_name().to_string(),
            }),
        },
        UnaryOp::Not => match src {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            _ => Err(VmError::BranchTypeMismatch {
                got: src.type_name().to_string(),
            }),
        },
    }
}

fn exec_add(lhs: &Value, rhs: &Value) -> Result<Value, VmError> {
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
        _ => Err(VmError::BranchTypeMismatch {
            got: format!("{} + {}", lhs.type_name(), rhs.type_name()),
        }),
    }
}

fn exec_arith(
    lhs: &Value,
    rhs: &Value,
    int_op: impl Fn(i64, i64) -> i64,
    float_op: impl Fn(f64, f64) -> f64,
) -> Result<Value, VmError> {
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(int_op(*a, *b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(float_op(*a, *b))),
        _ => Err(VmError::BranchTypeMismatch {
            got: format!("{} op {}", lhs.type_name(), rhs.type_name()),
        }),
    }
}

fn exec_div(lhs: &Value, rhs: &Value) -> Result<Value, VmError> {
    match (lhs, rhs) {
        (Value::Int(_), Value::Int(0)) => Err(VmError::DivisionByZero),
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        _ => Err(VmError::BranchTypeMismatch {
            got: format!("{} / {}", lhs.type_name(), rhs.type_name()),
        }),
    }
}

fn exec_mod(lhs: &Value, rhs: &Value) -> Result<Value, VmError> {
    match (lhs, rhs) {
        (Value::Int(_), Value::Int(0)) => Err(VmError::DivisionByZero),
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
        _ => Err(VmError::BranchTypeMismatch {
            got: format!("{} % {}", lhs.type_name(), rhs.type_name()),
        }),
    }
}

fn exec_cmp(
    lhs: &Value,
    rhs: &Value,
    pred: impl Fn(std::cmp::Ordering) -> bool,
) -> Result<Value, VmError> {
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(pred(a.cmp(b)))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(pred(
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
        ))),
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(pred(a.cmp(b)))),
        (Value::String(a), Value::String(b)) => Ok(Value::Bool(pred(a.cmp(b)))),
        _ => Err(VmError::BranchTypeMismatch {
            got: format!("{} cmp {}", lhs.type_name(), rhs.type_name()),
        }),
    }
}

fn exec_logical_and(lhs: &Value, rhs: &Value) -> Result<Value, VmError> {
    match (lhs, rhs) {
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
        _ => Err(VmError::BranchTypeMismatch {
            got: format!("{} && {}", lhs.type_name(), rhs.type_name()),
        }),
    }
}

fn exec_logical_or(lhs: &Value, rhs: &Value) -> Result<Value, VmError> {
    match (lhs, rhs) {
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
        _ => Err(VmError::BranchTypeMismatch {
            got: format!("{} || {}", lhs.type_name(), rhs.type_name()),
        }),
    }
}

fn exec_bitwise(lhs: &Value, rhs: &Value, op: impl Fn(i64, i64) -> i64) -> Result<Value, VmError> {
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(op(*a, *b))),
        _ => Err(VmError::BranchTypeMismatch {
            got: format!("{} bitwise {}", lhs.type_name(), rhs.type_name()),
        }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_add_int() {
        assert_eq!(
            exec_binop(BinOp::Add, &Value::Int(3), &Value::Int(4)).unwrap(),
            Value::Int(7)
        );
    }

    #[test]
    fn test_add_float() {
        assert_eq!(
            exec_binop(BinOp::Add, &Value::Float(1.5), &Value::Float(2.5)).unwrap(),
            Value::Float(4.0)
        );
    }

    #[test]
    fn test_add_string() {
        assert_eq!(
            exec_binop(
                BinOp::Add,
                &Value::String("a".into()),
                &Value::String("b".into())
            )
            .unwrap(),
            Value::String("ab".into())
        );
    }

    #[test]
    fn test_sub_int() {
        assert_eq!(
            exec_binop(BinOp::Sub, &Value::Int(10), &Value::Int(3)).unwrap(),
            Value::Int(7)
        );
    }

    #[test]
    fn test_mul_int() {
        assert_eq!(
            exec_binop(BinOp::Mul, &Value::Int(3), &Value::Int(4)).unwrap(),
            Value::Int(12)
        );
    }

    #[test]
    fn test_div_int() {
        assert_eq!(
            exec_binop(BinOp::Div, &Value::Int(10), &Value::Int(3)).unwrap(),
            Value::Int(3)
        );
    }

    #[test]
    fn test_div_by_zero() {
        assert!(matches!(
            exec_binop(BinOp::Div, &Value::Int(1), &Value::Int(0)),
            Err(VmError::DivisionByZero)
        ));
    }

    #[test]
    fn test_mod_int() {
        assert_eq!(
            exec_binop(BinOp::Mod, &Value::Int(10), &Value::Int(3)).unwrap(),
            Value::Int(1)
        );
    }

    #[test]
    fn test_eq_int() {
        assert_eq!(
            exec_binop(BinOp::Eq, &Value::Int(5), &Value::Int(5)).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            exec_binop(BinOp::Eq, &Value::Int(5), &Value::Int(6)).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_ne_int() {
        assert_eq!(
            exec_binop(BinOp::Ne, &Value::Int(5), &Value::Int(6)).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_lt_int() {
        assert_eq!(
            exec_binop(BinOp::Lt, &Value::Int(3), &Value::Int(5)).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_ge_int() {
        assert_eq!(
            exec_binop(BinOp::Ge, &Value::Int(5), &Value::Int(5)).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_and() {
        assert_eq!(
            exec_binop(BinOp::And, &Value::Bool(true), &Value::Bool(false)).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_or() {
        assert_eq!(
            exec_binop(BinOp::Or, &Value::Bool(false), &Value::Bool(true)).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_shl() {
        assert_eq!(
            exec_binop(BinOp::Shl, &Value::Int(1), &Value::Int(3)).unwrap(),
            Value::Int(8)
        );
    }

    #[test]
    fn test_bitwise_and() {
        assert_eq!(
            exec_binop(BinOp::BitAnd, &Value::Int(0b1100), &Value::Int(0b1010)).unwrap(),
            Value::Int(0b1000)
        );
    }

    #[test]
    fn test_neg_int() {
        assert_eq!(
            exec_unaryop(UnaryOp::Neg, &Value::Int(5)).unwrap(),
            Value::Int(-5)
        );
    }

    #[test]
    fn test_not_bool() {
        assert_eq!(
            exec_unaryop(UnaryOp::Not, &Value::Bool(true)).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_type_mismatch() {
        assert!(exec_binop(BinOp::Add, &Value::Int(1), &Value::Bool(true)).is_err());
    }
}
