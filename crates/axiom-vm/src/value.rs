//! Runtime value representation.

use std::fmt;

/// A runtime value in the VM.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Unit,
    Struct {
        type_name: String,
        fields: Vec<(String, Value)>,
    },
    Enum {
        type_name: String,
        variant: String,
        payload: Vec<Value>,
    },
    List(Vec<Value>),
    HeapPtr(usize),
}

impl Value {
    /// Convert from an IR constant.
    pub fn from_const(c: &axiom_ir::IrConst) -> Self {
        match c {
            axiom_ir::IrConst::Int(n) => Value::Int(*n),
            axiom_ir::IrConst::Float(f) => Value::Float(*f),
            axiom_ir::IrConst::Bool(b) => Value::Bool(*b),
            axiom_ir::IrConst::String(s) => Value::String(s.clone()),
            axiom_ir::IrConst::Unit => Value::Unit,
        }
    }

    /// Display name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::String(_) => "String",
            Value::Unit => "Unit",
            Value::Struct { .. } => "Struct",
            Value::Enum { .. } => "Enum",
            Value::List(_) => "List",
            Value::HeapPtr(_) => "HeapPtr",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::String(s) => write!(f, "{s}"),
            Value::Unit => write!(f, "()"),
            Value::Struct { type_name, fields } => {
                write!(f, "{type_name} {{")?;
                for (i, (name, val)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {val}")?;
                }
                write!(f, "}}")
            }
            Value::Enum {
                type_name,
                variant,
                payload,
            } => {
                write!(f, "{type_name}.{variant}")?;
                if !payload.is_empty() {
                    write!(f, "(")?;
                    for (i, val) in payload.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{val}")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Value::List(items) => {
                write!(f, "[")?;
                for (i, val) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{val}")?;
                }
                write!(f, "]")
            }
            Value::HeapPtr(addr) => write!(f, "HeapPtr({addr})"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::HeapPtr(a), Value::HeapPtr(b)) => a == b,
            // Struct/Enum: compare by type + fields/variant+payload
            (
                Value::Struct {
                    type_name: tn1,
                    fields: f1,
                },
                Value::Struct {
                    type_name: tn2,
                    fields: f2,
                },
            ) => tn1 == tn2 && f1 == f2,
            (
                Value::Enum {
                    type_name: tn1,
                    variant: v1,
                    payload: p1,
                },
                Value::Enum {
                    type_name: tn2,
                    variant: v2,
                    payload: p2,
                },
            ) => tn1 == tn2 && v1 == v2 && p1 == p2,
            _ => false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_value_display_int() {
        assert_eq!(Value::Int(42).to_string(), "42");
    }

    #[test]
    fn test_value_display_float() {
        assert_eq!(Value::Float(3.25).to_string(), "3.25");
    }

    #[test]
    fn test_value_display_bool() {
        assert_eq!(Value::Bool(true).to_string(), "true");
    }

    #[test]
    fn test_value_display_string() {
        assert_eq!(Value::String("hi".into()).to_string(), "hi");
    }

    #[test]
    fn test_value_display_unit() {
        assert_eq!(Value::Unit.to_string(), "()");
    }

    #[test]
    fn test_value_display_struct() {
        let v = Value::Struct {
            type_name: "Point".into(),
            fields: vec![("x".into(), Value::Int(1)), ("y".into(), Value::Int(2))],
        };
        assert_eq!(v.to_string(), "Point {x: 1, y: 2}");
    }

    #[test]
    fn test_value_display_enum() {
        let v = Value::Enum {
            type_name: "Shape".into(),
            variant: "Circle".into(),
            payload: vec![Value::Float(5.0)],
        };
        assert_eq!(v.to_string(), "Shape.Circle(5)");
    }

    #[test]
    fn test_value_display_list() {
        let v = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(v.to_string(), "[1, 2, 3]");
    }

    #[test]
    fn test_value_display_heapptr() {
        assert_eq!(Value::HeapPtr(7).to_string(), "HeapPtr(7)");
    }

    #[test]
    fn test_value_from_const() {
        assert_eq!(
            Value::from_const(&axiom_ir::IrConst::Int(99)),
            Value::Int(99)
        );
        assert_eq!(Value::from_const(&axiom_ir::IrConst::Unit), Value::Unit);
    }

    #[test]
    fn test_value_type_name() {
        assert_eq!(Value::Int(0).type_name(), "Int");
        assert_eq!(Value::Bool(false).type_name(), "Bool");
        assert_eq!(Value::List(vec![]).type_name(), "List");
    }

    #[test]
    fn test_value_eq_same() {
        assert_eq!(Value::Int(5), Value::Int(5));
        assert_eq!(Value::Bool(true), Value::Bool(true));
        assert_eq!(Value::Unit, Value::Unit);
    }

    #[test]
    fn test_value_eq_different_type() {
        assert_ne!(Value::Int(1), Value::Bool(true));
        assert_ne!(Value::Unit, Value::Int(0));
    }

    #[test]
    fn test_value_eq_struct() {
        let a = Value::Struct {
            type_name: "P".into(),
            fields: vec![("x".into(), Value::Int(1))],
        };
        let b = Value::Struct {
            type_name: "P".into(),
            fields: vec![("x".into(), Value::Int(1))],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_value_eq_enum() {
        let a = Value::Enum {
            type_name: "E".into(),
            variant: "A".into(),
            payload: vec![],
        };
        let b = Value::Enum {
            type_name: "E".into(),
            variant: "B".into(),
            payload: vec![],
        };
        assert_ne!(a, b);
    }
}
