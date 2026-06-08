//! Compiler-registered *floor* methods for built-in types.
//!
//! The core traits (Deinit/Equatable/Hashable/Ord) and their primitive impls
//! are now ordinary library code in `stdlib/core/*.ax`. What remains here are
//! the irreducible floor methods the VM implements and the library forwards to:
//!   - `String::len`, `String::as_bytes` — the String→Bytes/length floor
//!   - `{Int,Float,Bool,String}::hash_raw` — the scalar-hash floor (Hashable)
//!   - `List::push`, `Map::set` — collection intrinsic stand-ins (retired in
//!     Phase D once `HeapBuffer<T>` lands)

use super::{ImplInfo, TypeChecker};
use resolver::{Block, CallingConvention, FnDef, HirId, HirTy, HirTypeParam, Param, Visibility};
use std::collections::HashMap;

/// All primitive type names that get auto-impls for Equatable/Hashable/Ord.
const PRIMITIVE_TYPES: &[&str] = &["Int", "Float", "Bool", "String"];

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
        lang_tag: None,
        intrinsic_tag: None,
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

impl TypeChecker {
    /// Register inherent methods for built-in types (String floor ops).
    ///
    /// Neither `List` nor `Map` has an entry here any more: both are real
    /// library code in `stdlib/std/collections/*.ax`, built on the
    /// `HeapBuffer<T>` floor ops (migrations M6/M7). What remains is the
    /// irreducible String/Bytes/hash floor.
    pub(super) fn register_builtin_methods(&mut self) {
        self.register_string_methods();
        self.register_hash_methods();
    }

    /// Register the `hash_raw` floor method on each primitive: `fn hash_raw(let
    /// self) -> Int`. This is the irreducible scalar-hash primitive (like
    /// `String::as_bytes`), dispatched in the VM. The `Hashable::hash` impls in
    /// `core/primitives.ax` + `core/string.ax` forward to it.
    fn register_hash_methods(&mut self) {
        let int_ty = HirTy::Named(resolver::NameRef::unresolved("Int"));
        for type_name in PRIMITIVE_TYPES {
            let self_ty = HirTy::Named(resolver::NameRef::unresolved(*type_name));
            let methods = vec![make_fn(
                "hash_raw",
                vec![],
                vec![self_param(CallingConvention::Let, self_ty)],
                Some(int_ty.clone()),
            )];
            self.impl_table.push(ImplInfo {
                trait_name: None,
                type_name: type_name.to_string(),
                methods,
                subscripts: vec![],
                type_params: vec![],
                type_param_bounds: HashMap::new(),
            });
        }
    }

    fn register_string_methods(&mut self) {
        let string_ty = HirTy::Instance(resolver::InstanceTy {
            name: resolver::NameRef::unresolved("String"),
            args: vec![],
        });
        let bytes_ty = HirTy::Instance(resolver::InstanceTy {
            name: resolver::NameRef::unresolved("Bytes"),
            args: vec![],
        });

        // `as_bytes` is the irreducible String→Bytes floor. `len` is no longer
        // here — it is library code in core/string.ax that calls
        // `self.as_bytes().len()`, bottoming out on the `Bytes::len` floor below.
        let methods = vec![make_fn(
            "as_bytes",
            vec![],
            vec![self_param(CallingConvention::Let, string_ty)],
            Some(bytes_ty),
        )];

        self.impl_table.push(ImplInfo {
            trait_name: None,
            type_name: "String".to_string(),
            methods,
            subscripts: vec![],
            type_params: vec![],
            type_param_bounds: HashMap::new(),
        });

        self.register_bytes_methods();
    }

    /// Register the `Bytes::len` floor method: `fn len(let self: Bytes) -> Int`.
    /// `Bytes` is the platform byte-buffer; its length is the irreducible length
    /// floor that `String::len` (library code) builds on.
    fn register_bytes_methods(&mut self) {
        let bytes_ty = HirTy::Instance(resolver::InstanceTy {
            name: resolver::NameRef::unresolved("Bytes"),
            args: vec![],
        });
        let int_ty = HirTy::Named(resolver::NameRef::unresolved("Int"));
        let methods = vec![make_fn(
            "len",
            vec![],
            vec![self_param(CallingConvention::Let, bytes_ty)],
            Some(int_ty),
        )];
        self.impl_table.push(ImplInfo {
            trait_name: None,
            type_name: "Bytes".to_string(),
            methods,
            subscripts: vec![],
            type_params: vec![],
            type_param_bounds: HashMap::new(),
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use parser::ast::AstNode;
    use resolver::lower;

    fn make_checker(source: &str) -> TypeChecker {
        let result = parser::parse(source);
        let root = parser::ast::SourceFile::cast(result.tree).unwrap();
        let hir = lower(&root, source, None);
        TypeChecker::new(hir, resolver::LangItems::default())
    }

    #[test]
    fn test_hash_raw_method_registered_for_primitives() {
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_methods();
        for ty in ["Int", "Float", "Bool", "String"] {
            let has_hash = checker
                .impl_table
                .iter()
                .filter(|i| i.trait_name.is_none() && i.type_name == ty)
                .any(|i| i.methods.iter().any(|m| m.name == "hash_raw"));
            assert!(has_hash, "missing hash_raw floor method for {ty}");
        }
    }

    #[test]
    fn test_supertraits_collected_from_trait_decl_syntax() {
        // `trait X: A + B { .. }` registers A and B as supertraits, sourced
        // from the declaration's supertrait clause (collect_trait_defs).
        let mut checker = make_checker(
            "trait Equatable {}\ntrait Hashable: Equatable {}\ntrait Ord: Equatable + Hashable {}",
        );
        checker.collect_pass();
        assert_eq!(
            checker.trait_registry.get("Hashable").unwrap().supertraits,
            vec!["Equatable"]
        );
        assert_eq!(
            checker.trait_registry.get("Ord").unwrap().supertraits,
            vec!["Equatable", "Hashable"]
        );
    }

    #[test]
    fn test_no_builtin_list_methods_registered() {
        // `List` is fully library code now (M6): no compiler-registered methods.
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_methods();
        assert!(
            !checker
                .impl_table
                .iter()
                .any(|i| i.trait_name.is_none() && i.type_name == "List"),
            "List should have no compiler-registered impl methods"
        );
    }

    #[test]
    fn test_no_builtin_map_methods_registered() {
        // `Map` is fully library code now (M7): no compiler-registered methods.
        let mut checker = make_checker("fn main() {}");
        checker.register_builtin_methods();
        assert!(
            !checker
                .impl_table
                .iter()
                .any(|i| i.trait_name.is_none() && i.type_name == "Map"),
            "Map should have no compiler-registered impl methods"
        );
    }
}
