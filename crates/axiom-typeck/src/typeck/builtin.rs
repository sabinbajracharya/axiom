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
use std::collections::HashMap;

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
        module_path: String::new(),
        visibility: Visibility::Private,
        type_params,
        params,
        return_type,
        body: Block {
            id: HirId(0),
            stmts: vec![],
            tail: None,
        },
        extern_abi: None,
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

    /// Register inherent methods for built-in types (List, Map, String).
    pub(super) fn register_builtin_methods(&mut self) {
        self.register_list_methods();
        self.register_map_methods();
        self.register_string_methods();
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
        let tps = vec![tp];

        // Only `push` remains as a compiler intrinsic. The other List methods
        // (count, is_empty, capacity, subscript) are defined in stdlib/std/collections/list.ax.
        let methods = vec![make_fn(
            "push",
            tps,
            vec![
                self_param(CallingConvention::Inout, list_ty),
                named_param(HirId(102), CallingConvention::Sink, "element", t_ty),
            ],
            None,
        )];

        self.impl_table.push(ImplInfo {
            trait_name: None,
            type_name: "List".to_string(),
            methods,
            subscripts: vec![],
            type_params: vec![("T".to_string(), HirId(100))],
            type_param_bounds: HashMap::new(),
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
        let tps = vec![k_tp, v_tp];

        // Only `set` remains as a compiler intrinsic. The other Map methods
        // (get, has, count, is_empty, subscript) are defined in stdlib/std/collections/map.ax.
        let methods = vec![make_fn(
            "set",
            tps,
            vec![
                self_param(CallingConvention::Inout, map_ty),
                named_param(HirId(203), CallingConvention::Sink, "key", k_ty),
                named_param(HirId(204), CallingConvention::Sink, "value", v_ty),
            ],
            None,
        )];

        self.impl_table.push(ImplInfo {
            trait_name: None,
            type_name: "Map".to_string(),
            methods,
            subscripts: vec![],
            type_params: vec![("K".to_string(), HirId(200)), ("V".to_string(), HirId(201))],
            type_param_bounds: HashMap::new(),
        });
    }

    fn register_string_methods(&mut self) {
        let string_ty = HirTy::Instance(axiom_hir::InstanceTy {
            name: axiom_hir::NameRef::unresolved("String"),
            args: vec![],
        });
        let int_ty = HirTy::Named(axiom_hir::NameRef::unresolved("Int"));
        let bytes_ty = HirTy::Instance(axiom_hir::InstanceTy {
            name: axiom_hir::NameRef::unresolved("Bytes"),
            args: vec![],
        });

        let methods = vec![
            make_fn(
                "len",
                vec![],
                vec![self_param(CallingConvention::Let, string_ty.clone())],
                Some(int_ty),
            ),
            make_fn(
                "as_bytes",
                vec![],
                vec![self_param(CallingConvention::Let, string_ty)],
                Some(bytes_ty),
            ),
        ];

        self.impl_table.push(ImplInfo {
            trait_name: None,
            type_name: "String".to_string(),
            methods,
            subscripts: vec![],
            type_params: vec![],
            type_param_bounds: HashMap::new(),
        });
    }

    /// Register auto-implementations for built-in traits.
    pub(super) fn register_builtin_impls(&mut self) {
        for type_name in ALL_TYPES {
            self.impl_table.push(ImplInfo {
                trait_name: Some("Deinit".to_string()),
                type_name: type_name.to_string(),
                methods: vec![],
                subscripts: vec![],
                type_params: vec![],
                type_param_bounds: HashMap::new(),
            });
        }

        for type_name in PRIMITIVE_TYPES {
            for trait_name in &["Equatable", "Hashable", "Ord"] {
                self.impl_table.push(ImplInfo {
                    trait_name: Some(trait_name.to_string()),
                    type_name: type_name.to_string(),
                    methods: vec![],
                    subscripts: vec![],
                    type_params: vec![],
                    type_param_bounds: HashMap::new(),
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
        let hir = lower(&root, source, None);
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
        checker.register_builtin_methods();
        let list_impl = checker
            .impl_table
            .iter()
            .find(|i| i.trait_name.is_none() && i.type_name == "List")
            .unwrap();
        let names: Vec<_> = list_impl.methods.iter().map(|m| m.name.as_str()).collect();
        // Only `push` remains as compiler intrinsic. count/is_empty/capacity
        // are now defined in stdlib/std/collections/list.ax.
        assert_eq!(names, vec!["push"]);
    }

    #[test]
    fn test_builtin_map_methods_registered() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_methods();
        let map_impl = checker
            .impl_table
            .iter()
            .find(|i| i.trait_name.is_none() && i.type_name == "Map")
            .unwrap();
        let names: Vec<_> = map_impl.methods.iter().map(|m| m.name.as_str()).collect();
        // Only `set` remains as compiler intrinsic. get/has/count/is_empty
        // are now defined in stdlib/std/collections/map.ax.
        assert_eq!(names, vec!["set"]);
    }
}
