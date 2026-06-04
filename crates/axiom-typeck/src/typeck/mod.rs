//! The type checker: walks the HIR, assigns types to every expression and
//! statement, and collects type diagnostics.
//!
//! Two-pass design (per `docs/typeck-testing.md` §4.4):
//!   Pass 1 — Collect: register fn signatures, struct definitions, and enum
//!     definitions in the type environment. This allows forward references.
//!   Pass 2 — Check: walk fn bodies, type-checking each expression against the
//!     environment.
//!
//! Bidirectional typing (per §4.1):
//!   - `infer(expr) → Ty`: compute the type from subexpressions and the env.
//!   - `check(expr, expected) → Ty`: verify against an expected type.
//!
//! On error, return `Ty::Error` and emit a diagnostic. `Ty::Error` is sticky
//! (does not cascade additional diagnostics from subexpressions).

mod collect;
mod control;
mod helpers;
mod infer;
mod stmt;
mod unify;

use crate::error::TypeDiagnostic;
use crate::thir::{Thir, TypeMap};

use axiom_hir::*;
use std::collections::HashMap;

// ── Public entry point ────────────────────────────────────────────────────────

/// Type-check an HIR, producing a THIR (HIR + type map + diagnostics).
/// The HIR is consumed (moved) — the THIR owns it.
/// Never panics on user-reachable input. Returns a Thir even if
/// type errors exist; diagnostics are in `thir.diagnostics`.
pub fn check(hir: Hir) -> Thir {
    let mut checker = TypeChecker::new(hir);
    checker.collect_pass();
    checker.check_pass();
    Thir {
        hir: checker.hir,
        types: checker.types,
        diagnostics: checker.diagnostics,
    }
}

// ── The type checker ──────────────────────────────────────────────────────────

struct TypeChecker {
    hir: Hir,
    types: TypeMap,
    diagnostics: Vec<TypeDiagnostic>,
    env: TypeEnv,
    /// Tracks which HirIds correspond to mutable bindings (var, not val).
    mutability: HashMap<HirId, Mutability>,
    /// Stack of break-type collectors, one per enclosing loop.
    /// Each entry collects the types of `break value` expressions within that loop.
    loop_break_types: Vec<Vec<crate::types::Ty>>,
    /// Type parameters of the function currently being collected or checked.
    /// Set before resolving param/return types, cleared after.
    /// Empty = not inside a generic function.
    current_type_params: Vec<(String, HirId)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mutability {
    Immutable,
    Mutable,
}

// ── Type environment ──────────────────────────────────────────────────────────

/// The type environment: a stack of scopes mapping names to types.
struct TypeEnv {
    scopes: Vec<Scope>,
}

struct Scope {
    bindings: HashMap<String, BindingInfo>,
}

struct BindingInfo {
    ty: crate::types::Ty,
    _def_id: DefId,
    mutability: Mutability,
}

struct StructInfo {
    name: String,
    def_id: DefId,
    fields: Vec<FieldInfo>,
}

struct FieldInfo {
    name: String,
    ty: crate::types::Ty,
}

struct EnumInfo {
    name: String,
    def_id: DefId,
    variants: Vec<VariantInfo>,
}

struct VariantInfo {
    name: String,
    def_id: DefId,
    payload: Vec<crate::types::Ty>,
}

impl TypeEnv {
    fn new() -> Self {
        TypeEnv {
            scopes: vec![Scope {
                bindings: HashMap::new(),
            }],
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope {
            bindings: HashMap::new(),
        });
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn define(
        &mut self,
        name: String,
        ty: crate::types::Ty,
        def_id: DefId,
        mutability: Mutability,
    ) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(
                name,
                BindingInfo {
                    ty,
                    _def_id: def_id,
                    mutability,
                },
            );
        }
    }

    fn lookup(&self, name: &str) -> Option<&BindingInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.bindings.get(name) {
                return Some(info);
            }
        }
        None
    }
}

impl TypeChecker {
    fn new(hir: Hir) -> Self {
        TypeChecker {
            hir,
            types: TypeMap::new(),
            diagnostics: Vec::new(),
            env: TypeEnv::new(),
            mutability: HashMap::new(),
            loop_break_types: Vec::new(),
            current_type_params: Vec::new(),
        }
    }

    fn check_pass(&mut self) {
        // Clone required: check_fn_body borrows self mutably while iterating.
        for item in self.hir.items.clone() {
            if let Item::FnDef(f) = item {
                self.check_fn_body(&f);
            }
        }
    }

    fn check_fn_body(&mut self, f: &FnDef) {
        // Set type param scope so resolve_hir_ty can resolve T, U, etc.
        self.current_type_params = f
            .type_params
            .iter()
            .map(|tp| (tp.name.clone(), tp.id))
            .collect();
        self.env.push_scope();
        for param in &f.params {
            let param_type = param
                .ty
                .as_ref()
                .map(|t| self.resolve_hir_ty(t))
                .unwrap_or(crate::types::Ty::Error);
            let mutability = Mutability::Immutable;
            self.env
                .define(param.name.clone(), param_type.clone(), param.id, mutability);
            self.types.insert(param.id, param_type);
            self.mutability.insert(param.id, mutability);
        }
        let return_type = f
            .return_type
            .as_ref()
            .map(|t| self.resolve_hir_ty(t))
            .unwrap_or(crate::types::Ty::Unit);

        let body_type = self.check_block(&f.body, &return_type);

        if !helpers::is_error(&body_type)
            && !helpers::is_error(&return_type)
            && body_type != return_type
        {
            self.emit(TypeDiagnostic::ReturnTypeMismatch {
                expected: return_type.to_string(),
                found: body_type.to_string(),
                span: self.span_for(f.id),
            });
            self.types.insert(f.id, crate::types::Ty::Error);
        }

        let fn_ty = crate::types::Ty::Fn(crate::types::FnTy {
            params: f
                .params
                .iter()
                .map(|p| {
                    self.types
                        .get(&p.id)
                        .cloned()
                        .unwrap_or(crate::types::Ty::Error)
                })
                .collect(),
            return_type: Box::new(return_type.clone()),
        });
        self.types.insert(f.id, fn_ty);
        self.env.pop_scope();
        self.current_type_params.clear();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axiom_hir::lower;
    use axiom_parser::ast::AstNode;

    fn check_source(source: &str) -> Thir {
        let result = axiom_parser::parse(source);
        let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
        let hir = lower(&root, source);
        check(hir)
    }

    #[test]
    fn test_infer_int_literal() {
        let thir = check_source("fn main() { val x = 42 }");
        let has_int = thir.types.values().any(|t| *t == crate::types::Ty::Int);
        assert!(
            has_int,
            "expected Int type somewhere, got: {:?}",
            thir.types
        );
    }

    #[test]
    fn test_infer_string_literal() {
        let thir = check_source("fn main() { print(\"hello\") }");
        let has_string = thir
            .types
            .values()
            .any(|t| matches!(t, crate::types::Ty::String));
        assert!(has_string, "expected String type somewhere");
    }

    #[test]
    fn test_infer_bin_op_add() {
        let thir = check_source("fn main() { val x = 1 + 2 }");
        let has_int = thir.types.values().any(|t| *t == crate::types::Ty::Int);
        assert!(has_int, "expected Int type from addition");
    }

    #[test]
    fn test_type_mismatch_bin_op() {
        let thir = check_source("fn main() { val x = 1 + 2.0 }");
        assert!(
            thir.diagnostics
                .iter()
                .any(|d| d.kind() == "bin_op_mismatch"),
            "expected bin op mismatch diagnostic, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_fn_call_with_params() {
        // `main` has no explicit return type (defaults to Unit), but the body
        // produces Int via `add(1, 2)`. That is a real type mismatch now that
        // block tail expressions are properly tracked.
        let thir =
            check_source("fn add(a: Int, b: Int) -> Int { a + b } fn main() -> Int { add(1, 2) }");
        assert!(
            thir.diagnostics.iter().all(|d| d.kind() != "type_mismatch"),
            "unexpected type errors: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_fn_call_arity_mismatch() {
        let thir = check_source("fn add(a: Int, b: Int) -> Int { a + b } fn main() { add(1) }");
        assert!(
            thir.diagnostics
                .iter()
                .any(|d| d.kind() == "call_arity_mismatch"),
            "expected arity mismatch, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_struct_literal() {
        let thir = check_source(
            "struct Point { x: Float, y: Float }
fn main() { val p = Point { x: 1.0, y: 2.0 } }",
        );
        let has_struct = thir
            .types
            .values()
            .any(|t| matches!(t, crate::types::Ty::Struct(_)));
        assert!(has_struct, "expected Struct type");
    }

    #[test]
    fn test_enum_match() {
        let thir = check_source(
            "enum Shape { Circle(Float), Rect(Float, Float), Empty }
fn area(s: Shape) -> Float { match s { Circle(r) => 3.14 Rect(w, h) => 1.0 Empty => 0.0 } }",
        );
        let non_exhaustive: Vec<_> = thir
            .diagnostics
            .iter()
            .filter(|d| d.kind() == "non_exhaustive_match")
            .collect();
        assert!(
            non_exhaustive.is_empty(),
            "unexpected non-exhaustive match: {:?}",
            non_exhaustive
        );
    }

    #[test]
    fn test_non_exhaustive_match() {
        let thir = check_source(
            "enum Shape { Circle(Float), Rect(Float, Float) }
fn area(s: Shape) -> Float { match s { Circle(r) => r } }",
        );
        assert!(
            thir.diagnostics
                .iter()
                .any(|d| d.kind() == "non_exhaustive_match"),
            "expected non-exhaustive match diagnostic, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_assign_to_immutable() {
        let thir = check_source("fn main() { val x = 1 x = 2 }");
        assert!(
            thir.diagnostics
                .iter()
                .any(|d| d.kind() == "assign_to_immutable"),
            "expected assign_to_immutable diagnostic, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_if_branch_mismatch() {
        let thir = check_source("fn main() { val x: Float = if true { 1.0 } else { 2 } }");
        assert!(
            thir.diagnostics
                .iter()
                .any(|d| d.kind() == "type_mismatch" || d.kind() == "if_branch_mismatch"),
            "expected type mismatch diagnostic, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_unknown_field() {
        let thir = check_source(
            "struct Point { x: Float, y: Float }
fn main() { val p = Point { x: 1.0, y: 2.0 } val z = p.z }",
        );
        assert!(
            thir.diagnostics.iter().any(|d| d.kind() == "unknown_field"),
            "expected unknown_field diagnostic, got: {:?}",
            thir.diagnostics
        );
    }

    #[test]
    fn test_not_callable() {
        let thir = check_source("fn main() { val x = 1 x() }");
        assert!(
            thir.diagnostics.iter().any(|d| d.kind() == "not_callable"),
            "expected not_callable diagnostic, got: {:?}",
            thir.diagnostics
        );
    }
}
