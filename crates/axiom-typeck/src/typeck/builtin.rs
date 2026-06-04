//! Built-in traits, auto-implementations, and collection types.
//!
//! Four compiler-known traits are registered automatically:
//!   - `Deinit`   — every type
//!   - `Equatable` — primitives (Int, Float, Bool, String)
//!   - `Hashable`  — primitives (requires Equatable)
//!   - `Ord`       — primitives (requires Equatable)
//!
//! Built-in collection types: List<T>, Map<K, V>.

use super::{ImplInfo, TraitInfo, TraitMethodInfo, TypeChecker};
use crate::types::Ty;
use axiom_hir::{Block, CallingConvention, FnDef, HirId, HirTy, HirTypeParam, Param, Visibility};

/// All primitive type names that get auto-impls for Equatable/Hashable/Ord.
const PRIMITIVE_TYPES: &[&str] = &["Int", "Float", "Bool", "String"];

/// All type names that get auto-impls for Deinit (includes Unit).
const ALL_TYPES: &[&str] = &["Int", "Float", "Bool", "String", "Unit"];

/// Helper to build a synthetic FnDef for built-in methods.
fn make_fn(
    name: &str,
    type_params: Vec<HirTypeParam>,
    params: Vec<Param>,
    return_type: Option<HirTy>,
) -> FnDef {
    FnDef {
        id: HirId(0),
        name: name.to_string(),
        visibility: Visibility::Private,
        type_params,
        params,
        return_type,
        body: Block {
            id: HirId(0),
            stmts: vec![],
            tail: None,
        },
    }
}

/// Helper to build a self parameter.
fn self_param(convention: CallingConvention, ty: HirTy) -> Param {
    Param {
        id: HirId(101),
        convention,
        name: "self".to_string(),
        ty: Some(ty),
    }
}

/// Helper to build a named parameter.
fn named_param(id: HirId, convention: CallingConvention, name: &str, ty: HirTy) -> Param {
    Param {
        id,
        convention,
        name: name.to_string(),
        ty: Some(ty),
    }
}

impl TypeChecker {
    /// Register the four built-in trait definitions in the trait registry.
    pub(super) fn register_builtin_traits(&mut self) {
        self.trait_registry.insert(
            "Deinit".to_string(),
            TraitInfo {
                name: "Deinit".to_string(),
                def_id: HirId(0),
                required_methods: vec![TraitMethodInfo {
                    name: "drop".to_string(),
                    params: vec![],
                    return_type: Ty::Unit,
                }],
                default_methods: vec![],
                supertraits: vec![],
            },
        );

        self.trait_registry.insert(
            "Equatable".to_string(),
            TraitInfo {
                name: "Equatable".to_string(),
                def_id: HirId(0),
                required_methods: vec![TraitMethodInfo {
                    name: "eq".to_string(),
                    params: vec![],
                    return_type: Ty::Bool,
                }],
                default_methods: vec![],
                supertraits: vec![],
            },
        );

        self.trait_registry.insert(
            "Hashable".to_string(),
            TraitInfo {
                name: "Hashable".to_string(),
                def_id: HirId(0),
                required_methods: vec![TraitMethodInfo {
                    name: "hash".to_string(),
                    params: vec![],
                    return_type: Ty::Int,
                }],
                default_methods: vec![],
                supertraits: vec!["Equatable".to_string()],
            },
        );

        self.trait_registry.insert(
            "Ord".to_string(),
            TraitInfo {
                name: "Ord".to_string(),
                def_id: HirId(0),
                required_methods: vec![TraitMethodInfo {
                    name: "cmp".to_string(),
                    params: vec![],
                    return_type: Ty::Unit,
                }],
                default_methods: vec![],
                supertraits: vec!["Equatable".to_string()],
            },
        );
    }

    /// Register built-in generic types (List, Map, HeapBuffer).
    pub(super) fn register_builtin_types(&mut self) {
        self.builtin_types.insert("List".to_string(), 1);
        self.builtin_types.insert("Map".to_string(), 2);
        self.builtin_types.insert("HeapBuffer".to_string(), 1);
    }

    /// Register inherent methods for built-in collection types.
    pub(super) fn register_builtin_methods(&mut self) {
        self.register_list_methods();
        self.register_map_methods();
    }

    fn register_list_methods(&mut self) {
        let tp = HirTypeParam {
            id: HirId(100),
            name: "T".to_string(),
            bounds: vec![],
        };
        let t_ty = HirTy::TypeParam(tp.clone());
        let list_ty = HirTy::Instance(axiom_hir::InstanceTy {
            name: axiom_hir::NameRef::unresolved("List"),
            args: vec![t_ty.clone()],
        });
        let int_ty = HirTy::Named(axiom_hir::NameRef::unresolved("Int"));
        let bool_ty = HirTy::Named(axiom_hir::NameRef::unresolved("Bool"));
        let tps = vec![tp];

        let methods = vec![
            make_fn(
                "push",
                tps.clone(),
                vec![
                    self_param(CallingConvention::Inout, list_ty.clone()),
                    named_param(HirId(102), CallingConvention::Sink, "element", t_ty),
                ],
                None,
            ),
            make_fn(
                "count",
                tps.clone(),
                vec![self_param(CallingConvention::Let, list_ty.clone())],
                Some(int_ty.clone()),
            ),
            make_fn(
                "is_empty",
                tps.clone(),
                vec![self_param(CallingConvention::Let, list_ty.clone())],
                Some(bool_ty),
            ),
            make_fn(
                "capacity",
                tps,
                vec![self_param(CallingConvention::Let, list_ty)],
                Some(int_ty),
            ),
        ];

        self.impl_table.push(ImplInfo {
            trait_name: None,
            type_name: "List".to_string(),
            methods,
        });
    }

    fn register_map_methods(&mut self) {
        let k_tp = HirTypeParam {
            id: HirId(200),
            name: "K".to_string(),
            bounds: vec![],
        };
        let v_tp = HirTypeParam {
            id: HirId(201),
            name: "V".to_string(),
            bounds: vec![],
        };
        let k_ty = HirTy::TypeParam(k_tp.clone());
        let v_ty = HirTy::TypeParam(v_tp.clone());
        let map_ty = HirTy::Instance(axiom_hir::InstanceTy {
            name: axiom_hir::NameRef::unresolved("Map"),
            args: vec![k_ty.clone(), v_ty.clone()],
        });
        let int_ty = HirTy::Named(axiom_hir::NameRef::unresolved("Int"));
        let bool_ty = HirTy::Named(axiom_hir::NameRef::unresolved("Bool"));
        let tps = vec![k_tp, v_tp];

        let self_let = self_param(CallingConvention::Let, map_ty.clone());
        let key_let = named_param(HirId(203), CallingConvention::Let, "key", k_ty.clone());
        let methods = vec![
            make_fn(
                "set",
                tps.clone(),
                vec![
                    self_param(CallingConvention::Inout, map_ty.clone()),
                    named_param(HirId(203), CallingConvention::Sink, "key", k_ty),
                    named_param(HirId(204), CallingConvention::Sink, "value", v_ty.clone()),
                ],
                None,
            ),
            make_fn(
                "get",
                tps.clone(),
                vec![self_let.clone(), key_let.clone()],
                Some(v_ty),
            ),
            make_fn(
                "has",
                tps.clone(),
                vec![self_let.clone(), key_let],
                Some(bool_ty.clone()),
            ),
            make_fn("count", tps.clone(), vec![self_let.clone()], Some(int_ty)),
            make_fn("is_empty", tps, vec![self_let], Some(bool_ty)),
        ];

        self.impl_table.push(ImplInfo {
            trait_name: None,
            type_name: "Map".to_string(),
            methods,
        });
    }

    /// Register auto-implementations for built-in traits.
    pub(super) fn register_builtin_impls(&mut self) {
        for type_name in ALL_TYPES {
            self.impl_table.push(ImplInfo {
                trait_name: Some("Deinit".to_string()),
                type_name: type_name.to_string(),
                methods: vec![],
            });
        }

        for type_name in PRIMITIVE_TYPES {
            for trait_name in &["Equatable", "Hashable", "Ord"] {
                self.impl_table.push(ImplInfo {
                    trait_name: Some(trait_name.to_string()),
                    type_name: type_name.to_string(),
                    methods: vec![],
                });
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axiom_hir::lower;
    use axiom_parser::ast::AstNode;

    fn make_checker(source: &str) -> TypeChecker {
        let result = axiom_parser::parse(source);
        let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
        let hir = lower(&root, source);
        TypeChecker::new(hir)
    }

    #[test]
    fn test_builtin_traits_registered() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_traits();
        assert!(checker.trait_registry.contains_key("Deinit"));
        assert!(checker.trait_registry.contains_key("Equatable"));
        assert!(checker.trait_registry.contains_key("Hashable"));
        assert!(checker.trait_registry.contains_key("Ord"));
    }

    #[test]
    fn test_builtin_deinit_auto_impl() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_impls();
        let deinit_impls: Vec<_> = checker
            .impl_table
            .iter()
            .filter(|i| i.trait_name.as_deref() == Some("Deinit"))
            .collect();
        assert_eq!(deinit_impls.len(), 5);
    }

    #[test]
    fn test_builtin_equatable_auto_impl() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_impls();
        let impls: Vec<_> = checker
            .impl_table
            .iter()
            .filter(|i| i.trait_name.as_deref() == Some("Equatable"))
            .collect();
        assert_eq!(impls.len(), 4);
    }

    #[test]
    fn test_builtin_hashable_auto_impl() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_impls();
        let impls: Vec<_> = checker
            .impl_table
            .iter()
            .filter(|i| i.trait_name.as_deref() == Some("Hashable"))
            .collect();
        assert_eq!(impls.len(), 4);
    }

    #[test]
    fn test_builtin_ord_auto_impl() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_impls();
        let impls: Vec<_> = checker
            .impl_table
            .iter()
            .filter(|i| i.trait_name.as_deref() == Some("Ord"))
            .collect();
        assert_eq!(impls.len(), 4);
    }

    #[test]
    fn test_builtin_supertrait_hashable_requires_equatable() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_traits();
        let hashable = checker.trait_registry.get("Hashable").unwrap();
        assert_eq!(hashable.supertraits, vec!["Equatable"]);
    }

    #[test]
    fn test_builtin_supertrait_ord_requires_equatable() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_traits();
        let ord = checker.trait_registry.get("Ord").unwrap();
        assert_eq!(ord.supertraits, vec!["Equatable"]);
    }

    #[test]
    fn test_builtin_list_methods_registered() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_types();
        checker.register_builtin_methods();
        let list_impl = checker
            .impl_table
            .iter()
            .find(|i| i.trait_name.is_none() && i.type_name == "List")
            .unwrap();
        let names: Vec<_> = list_impl.methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"push"));
        assert!(names.contains(&"count"));
        assert!(names.contains(&"is_empty"));
        assert!(names.contains(&"capacity"));
    }

    #[test]
    fn test_builtin_map_methods_registered() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_types();
        checker.register_builtin_methods();
        let map_impl = checker
            .impl_table
            .iter()
            .find(|i| i.trait_name.is_none() && i.type_name == "Map")
            .unwrap();
        let names: Vec<_> = map_impl.methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"set"));
        assert!(names.contains(&"get"));
        assert!(names.contains(&"has"));
        assert!(names.contains(&"count"));
        assert!(names.contains(&"is_empty"));
    }
}
