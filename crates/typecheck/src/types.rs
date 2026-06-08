//! The type universe for the Axiom type checker.
//! Every expression in the THIR carries one of these.
//!
//! Per `docs/typeck-testing.md` §3.2: the type checker's output assigns a `Ty`
//! to every expression and statement node. `Ty::Error` signals a type error
//! (always paired with a diagnostic — never silently propagated).
//!
//! Phase 2 adds `Ty::TypeParam` (for generic function signatures) and
//! `Ty::Instance` (for parameterized types like `Pair<Int, String>`).

use hir::DefId;
use std::fmt;

// ── The type universe ─────────────────────────────────────────────────────────

/// The types that expressions can produce.
///
/// Nominal (no structural typing for structs/enums). `Ty::Error` is sticky
/// in subexpressions — one error per root cause.
///
/// Phase 2 adds `TypeParam` and `Instance` for generics support.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Int,
    Float,
    Bool,
    String,
    Unit,
    Struct(StructTy),
    Enum(EnumTy),
    Fn(FnTy),
    Tuple(Vec<Ty>),
    /// A type parameter in a generic function signature (e.g., `T` in `fn id<T>(x: T) -> T`).
    TypeParam(TypeParamId),
    /// A parameterized type (e.g., `Pair<Int, String>`, `Option<Float>`).
    Instance(InstanceTy),
    /// A heap-allocated, runtime-sized buffer of homogeneous elements.
    /// Used by collection library types (List<T>, Map<K,V>) to store data.
    /// The inner `Ty` is the element type.
    HeapBuffer(Box<Ty>),
    Error,
}

/// A user-defined struct type, identified by name and DefId.
#[derive(Debug, Clone, PartialEq)]
pub struct StructTy {
    pub name: String,
    pub def_id: DefId,
}

/// A user-defined enum type, identified by name and DefId.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumTy {
    pub name: String,
    pub def_id: DefId,
}

/// A function type: `(param_types) -> return_type`.
#[derive(Debug, Clone, PartialEq)]
pub struct FnTy {
    pub params: Vec<Ty>,
    pub return_type: Box<Ty>,
}

/// Identity of a type parameter in a generic function/struct/enum.
/// Two `TypeParamId`s are equal iff they refer to the same declaration site.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeParamId {
    /// The parameter's name (e.g., "T", "U").
    pub name: String,
    /// 0-based index in the type parameter list.
    pub index: usize,
    /// HirId of the type parameter definition site.
    pub def_id: DefId,
}

/// A parameterized type: a named type applied to concrete type arguments.
/// Example: `Pair<Int, String>` → `InstanceTy { name: "Pair", args: [Int, String] }`.
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceTy {
    pub name: String,
    pub def_id: DefId,
    pub args: Vec<Ty>,
}

// ── Display ───────────────────────────────────────────────────────────────────

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "Int"),
            Ty::Float => write!(f, "Float"),
            Ty::Bool => write!(f, "Bool"),
            Ty::String => write!(f, "String"),
            Ty::Unit => write!(f, "Unit"),
            Ty::Struct(s) => write!(f, "{}", s.name),
            Ty::Enum(e) => write!(f, "{}", e.name),
            Ty::Fn(fn_ty) => write!(f, "{}", fn_ty),
            Ty::Tuple(elems) => {
                if elems.is_empty() {
                    write!(f, "()")
                } else {
                    write!(
                        f,
                        "({})",
                        elems
                            .iter()
                            .map(|t| t.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Ty::TypeParam(tp) => write!(f, "{}", tp.name),
            Ty::Instance(inst) => {
                write!(f, "{}<", inst.name)?;
                for (i, arg) in inst.args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ">")
            }
            Ty::HeapBuffer(inner) => write!(f, "HeapBuffer<{}>", inner),
            Ty::Error => write!(f, "///error///"),
        }
    }
}

impl fmt::Display for FnTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params = self
            .params
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "({}) -> {}", params, self.return_type)
    }
}

// ── Label (mirrors the HIR pattern: kind names come from code, not strings) ──

/// Short kind label for THIR dump nodes — mirrors `Ty` variant names
/// without hardcoding strings in the serializer.
#[allow(dead_code)]
pub fn label(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int => "Int",
        Ty::Float => "Float",
        Ty::Bool => "Bool",
        Ty::String => "String",
        Ty::Unit => "Unit",
        Ty::Struct(_) => "Struct",
        Ty::Enum(_) => "Enum",
        Ty::Fn(_) => "Fn",
        Ty::Tuple(_) => "Tuple",
        Ty::TypeParam(_) => "TypeParam",
        Ty::Instance(_) => "Instance",
        Ty::HeapBuffer(_) => "HeapBuffer",
        Ty::Error => "Error",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_display_primitives() {
        assert_eq!(Ty::Int.to_string(), "Int");
        assert_eq!(Ty::Float.to_string(), "Float");
        assert_eq!(Ty::Bool.to_string(), "Bool");
        assert_eq!(Ty::String.to_string(), "String");
        assert_eq!(Ty::Unit.to_string(), "Unit");
        assert_eq!(Ty::Error.to_string(), "///error///");
    }

    #[test]
    fn test_display_struct_ty() {
        let s = StructTy {
            name: "Point".to_string(),
            def_id: hir::HirId(5),
        };
        assert_eq!(Ty::Struct(s).to_string(), "Point");
    }

    #[test]
    fn test_display_enum_ty() {
        let e = EnumTy {
            name: "Shape".to_string(),
            def_id: hir::HirId(8),
        };
        assert_eq!(Ty::Enum(e).to_string(), "Shape");
    }

    #[test]
    fn test_display_fn_ty() {
        let f = FnTy {
            params: vec![Ty::Int, Ty::Float],
            return_type: Box::new(Ty::Bool),
        };
        assert_eq!(Ty::Fn(f).to_string(), "(Int, Float) -> Bool");
    }

    #[test]
    fn test_display_fn_ty_no_params() {
        let f = FnTy {
            params: vec![],
            return_type: Box::new(Ty::Unit),
        };
        assert_eq!(Ty::Fn(f).to_string(), "() -> Unit");
    }

    #[test]
    fn test_display_tuple_empty() {
        assert_eq!(Ty::Tuple(vec![]).to_string(), "()");
    }

    #[test]
    fn test_display_tuple_nonempty() {
        let t = Ty::Tuple(vec![Ty::Int, Ty::Float]);
        assert_eq!(t.to_string(), "(Int, Float)");
    }

    #[test]
    fn test_display_heap_buffer() {
        let hb = Ty::HeapBuffer(Box::new(Ty::Int));
        assert_eq!(hb.to_string(), "HeapBuffer<Int>");
    }

    #[test]
    fn test_label_heap_buffer() {
        let hb = Ty::HeapBuffer(Box::new(Ty::Bool));
        assert_eq!(label(&hb), "HeapBuffer");
    }

    #[test]
    fn test_label_matches_variant_names() {
        assert_eq!(label(&Ty::Int), "Int");
        assert_eq!(label(&Ty::Float), "Float");
        assert_eq!(label(&Ty::Bool), "Bool");
        assert_eq!(label(&Ty::String), "String");
        assert_eq!(label(&Ty::Unit), "Unit");
        assert_eq!(label(&Ty::Error), "Error");
        let s = StructTy {
            name: "X".to_string(),
            def_id: hir::HirId(0),
        };
        assert_eq!(label(&Ty::Struct(s)), "Struct");
        let e = EnumTy {
            name: "Y".to_string(),
            def_id: hir::HirId(0),
        };
        assert_eq!(label(&Ty::Enum(e)), "Enum");
        let f = FnTy {
            params: vec![],
            return_type: Box::new(Ty::Unit),
        };
        assert_eq!(label(&Ty::Fn(f)), "Fn");
        assert_eq!(label(&Ty::Tuple(vec![])), "Tuple");
    }
}
