//! Compiler intrinsics — the reverse of `@lang`: a stdlib-only attribute that
//! tells the compiler "this function signature is a declaration; the body is
//! supplied by the compiler as an IR instruction or VM builtin."
//!
//! See `docs/intrinsic-and-stdlib-identity.md` §2b–2d.

use crate::error::HirDiagnostic;
use crate::hir::{HirId, Item};
use axiom_lexer::Span;

// ── Intrinsic keys ────────────────────────────────────────────────────────────

/// `heap_alloc<T>(count: Int) -> [T]` — allocate a heap buffer.
pub const HEAP_ALLOC: &str = "heap_alloc";
/// `heap_free<T>(buf: [T])` — free a heap buffer.
pub const HEAP_FREE: &str = "heap_free";
/// `heap_get<T>(buf: [T], index: Int) -> T` — read an element from a heap buffer.
pub const HEAP_GET: &str = "heap_get";
/// `heap_set<T>(buf: [T], index: Int, value: T)` — write an element into a heap buffer.
pub const HEAP_SET: &str = "heap_set";

/// Every intrinsic key the compiler knows how to lower. A `@intrinsic` with an
/// unknown key produces an [`HirDiagnostic::UnknownIntrinsic`] diagnostic.
pub const KNOWN_INTRINSICS: &[&str] = &[HEAP_ALLOC, HEAP_FREE, HEAP_GET, HEAP_SET];

// ── Collection ────────────────────────────────────────────────────────────────

/// A `@intrinsic("key")` binding discovered in a module's HIR: the key and the
/// real `DefId` it annotates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntrinsicBinding {
    pub key: String,
    pub def_id: HirId,
}

/// Collect every `@intrinsic("…")` binding in a module's items — functions only.
/// Struct intrinsics are not yet supported (deferred; see the design doc §5).
pub fn collect_intrinsic_bindings(items: &[Item]) -> Vec<IntrinsicBinding> {
    let mut out = Vec::new();
    for item in items {
        if let Item::FnDef(f) = item {
            if let Some(key) = &f.intrinsic_tag {
                out.push(IntrinsicBinding {
                    key: key.clone(),
                    def_id: f.id,
                });
            }
        }
    }
    out
}

/// Validate intrinsic bindings against the known key list and module identity.
/// Returns diagnostics for bindings with unknown keys. The `@intrinsic` outside
/// stdlib check is done in `axiom_typeck::check_modules` using
/// `axm_stdlib::is_stdlib_module`.
pub fn validate_intrinsic_bindings(bindings: &[IntrinsicBinding]) -> Vec<HirDiagnostic> {
    let mut diags = Vec::new();
    for binding in bindings {
        if !KNOWN_INTRINSICS.contains(&binding.key.as_str()) {
            diags.push(HirDiagnostic::UnknownIntrinsic {
                key: binding.key.clone(),
                span: Span { lo: 0, hi: 0 },
            });
        }
    }
    diags
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_intrinsic_bindings_from_fn() {
        let source = "\
@intrinsic(\"heap_alloc\")
fn alloc<T>(count: Int) -> [T]
@intrinsic(\"heap_free\")
fn free<T>(buf: [T])
";
        let result = axiom_parser::parse(source);
        let root =
            <axiom_parser::ast::SourceFile as axiom_parser::ast::AstNode>::cast(result.tree)
                .unwrap();
        let (items, _defs, _diags, _nid) = crate::lower_structural(&root, source, 0);
        let bindings = collect_intrinsic_bindings(&items);
        let keys: Vec<&str> = bindings.iter().map(|b| b.key.as_str()).collect();
        assert!(keys.contains(&HEAP_ALLOC), "keys: {keys:?}");
        assert!(keys.contains(&HEAP_FREE), "keys: {keys:?}");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_collect_intrinsic_bindings_ignores_untagged_fn() {
        let source = "fn add(a: Int, b: Int) -> Int { a + b }";
        let result = axiom_parser::parse(source);
        let root =
            <axiom_parser::ast::SourceFile as axiom_parser::ast::AstNode>::cast(result.tree)
                .unwrap();
        let (items, _defs, _diags, _nid) = crate::lower_structural(&root, source, 0);
        let bindings = collect_intrinsic_bindings(&items);
        assert!(bindings.is_empty());
    }

    #[test]
    fn test_validate_intrinsic_bindings_known_key_is_clean() {
        let bindings = vec![IntrinsicBinding {
            key: HEAP_ALLOC.to_string(),
            def_id: HirId(1),
        }];
        let diags = validate_intrinsic_bindings(&bindings);
        assert!(diags.is_empty(), "unexpected diags: {diags:?}");
    }

    #[test]
    fn test_validate_intrinsic_bindings_unknown_key_emits_diagnostic() {
        let bindings = vec![IntrinsicBinding {
            key: "not_real".to_string(),
            def_id: HirId(1),
        }];
        let diags = validate_intrinsic_bindings(&bindings);
        assert_eq!(diags.len(), 1);
        assert!(matches!(
            &diags[0],
            HirDiagnostic::UnknownIntrinsic { key, .. } if key == "not_real"
        ));
    }
}
