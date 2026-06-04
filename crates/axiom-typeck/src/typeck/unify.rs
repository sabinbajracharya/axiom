//! Unification and substitution for generic type parameters.
//!
//! When a generic function like `fn id<T>(x: T) -> T` is called, the type
//! checker unifies the argument types with the parameter types to determine
//! the concrete type for each type parameter, then substitutes them in the
//! return type.

use super::TypeChecker;
use crate::types::{FnTy, Ty, TypeParamId};
use std::collections::HashMap;

/// Substitution map: type parameter → concrete type.
pub(crate) type Substitution = HashMap<TypeParamId, Ty>;

impl TypeChecker {
    /// Unify `actual` (the inferred argument type) with `expected` (the
    /// declared parameter type, which may contain type parameters).
    ///
    /// On success, `subst` is updated with new bindings.
    /// On failure, returns the type that was found (for error reporting).
    pub(crate) fn unify(
        &self,
        actual: &Ty,
        expected: &Ty,
        subst: &mut Substitution,
    ) -> Result<(), Ty> {
        match (actual, expected) {
            // Type parameter on the expected side: bind or check existing binding.
            (_, Ty::TypeParam(tp)) => {
                if let Some(bound) = subst.get(tp) {
                    if bound == actual {
                        Ok(())
                    } else {
                        Err(bound.clone())
                    }
                } else {
                    subst.insert(tp.clone(), actual.clone());
                    Ok(())
                }
            }
            // Function types: unify params and return type.
            (Ty::Fn(a), Ty::Fn(e)) => {
                if a.params.len() != e.params.len() {
                    return Err(actual.clone());
                }
                for (ap, ep) in a.params.iter().zip(e.params.iter()) {
                    self.unify(ap, ep, subst)?;
                }
                self.unify(&a.return_type, &e.return_type, subst)
            }
            // Tuples: unify element-wise.
            (Ty::Tuple(a), Ty::Tuple(e)) if a.len() == e.len() => {
                for (ae, ee) in a.iter().zip(e.iter()) {
                    self.unify(ae, ee, subst)?;
                }
                Ok(())
            }
            // Same concrete type: success.
            _ if actual == expected => Ok(()),
            // Mismatch.
            _ => Err(actual.clone()),
        }
    }

    /// Substitute all type parameters in `ty` using the given substitution map.
    /// Returns a new type with all known type parameters replaced.
    pub(crate) fn substitute(ty: &Ty, subst: &Substitution) -> Ty {
        match ty {
            Ty::TypeParam(tp) => subst.get(tp).cloned().unwrap_or_else(|| ty.clone()),
            Ty::Fn(f) => Ty::Fn(FnTy {
                params: f
                    .params
                    .iter()
                    .map(|p| Self::substitute(p, subst))
                    .collect(),
                return_type: Box::new(Self::substitute(&f.return_type, subst)),
            }),
            Ty::Tuple(elems) => {
                Ty::Tuple(elems.iter().map(|e| Self::substitute(e, subst)).collect())
            }
            // Concrete types (Int, Float, Bool, String, Unit, Struct, Enum, Error):
            // no substitution needed.
            _ => ty.clone(),
        }
    }

    /// Check if a type contains any unresolved type parameters.
    pub(crate) fn contains_type_param(ty: &Ty) -> bool {
        match ty {
            Ty::TypeParam(_) => true,
            Ty::Fn(f) => {
                f.params.iter().any(Self::contains_type_param)
                    || Self::contains_type_param(&f.return_type)
            }
            Ty::Tuple(elems) => elems.iter().any(Self::contains_type_param),
            _ => false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axiom_hir::HirId;

    fn tp(name: &str, index: usize) -> Ty {
        Ty::TypeParam(TypeParamId {
            name: name.to_string(),
            index,
            def_id: HirId(index),
        })
    }

    #[test]
    fn test_unify_type_param_with_concrete() {
        let checker = TypeChecker::new(axiom_hir::Hir {
            items: vec![],
            diagnostics: vec![],
        });
        let mut subst = Substitution::new();
        // unify Int with T → T = Int
        assert!(checker.unify(&Ty::Int, &tp("T", 0), &mut subst).is_ok());
        assert_eq!(
            subst.get(&TypeParamId {
                name: "T".to_string(),
                index: 0,
                def_id: HirId(0),
            }),
            Some(&Ty::Int)
        );
    }

    #[test]
    fn test_unify_type_param_conflict() {
        let checker = TypeChecker::new(axiom_hir::Hir {
            items: vec![],
            diagnostics: vec![],
        });
        let mut subst = Substitution::new();
        assert!(checker.unify(&Ty::Int, &tp("T", 0), &mut subst).is_ok());
        // T is bound to Int; unifying with Float should fail.
        assert!(checker.unify(&Ty::Float, &tp("T", 0), &mut subst).is_err());
    }

    #[test]
    fn test_unify_same_concrete() {
        let checker = TypeChecker::new(axiom_hir::Hir {
            items: vec![],
            diagnostics: vec![],
        });
        let mut subst = Substitution::new();
        assert!(checker.unify(&Ty::Int, &Ty::Int, &mut subst).is_ok());
        assert!(checker.unify(&Ty::Bool, &Ty::Float, &mut subst).is_err());
    }

    #[test]
    fn test_substitute_type_param() {
        let mut subst = Substitution::new();
        subst.insert(
            TypeParamId {
                name: "T".to_string(),
                index: 0,
                def_id: HirId(0),
            },
            Ty::Int,
        );
        assert_eq!(TypeChecker::substitute(&tp("T", 0), &subst), Ty::Int);
        // Unbound param stays as-is.
        assert_eq!(TypeChecker::substitute(&tp("U", 1), &subst), tp("U", 1));
    }

    #[test]
    fn test_substitute_fn_type() {
        let mut subst = Substitution::new();
        subst.insert(
            TypeParamId {
                name: "T".to_string(),
                index: 0,
                def_id: HirId(0),
            },
            Ty::Int,
        );
        let fn_ty = Ty::Fn(FnTy {
            params: vec![tp("T", 0)],
            return_type: Box::new(tp("T", 0)),
        });
        let result = TypeChecker::substitute(&fn_ty, &subst);
        assert_eq!(
            result,
            Ty::Fn(FnTy {
                params: vec![Ty::Int],
                return_type: Box::new(Ty::Int),
            })
        );
    }

    #[test]
    fn test_contains_type_param() {
        assert!(TypeChecker::contains_type_param(&tp("T", 0)));
        assert!(!TypeChecker::contains_type_param(&Ty::Int));
        let fn_ty = Ty::Fn(FnTy {
            params: vec![tp("T", 0)],
            return_type: Box::new(Ty::Int),
        });
        assert!(TypeChecker::contains_type_param(&fn_ty));
    }
}
