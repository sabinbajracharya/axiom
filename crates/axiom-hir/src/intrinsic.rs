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

/// Collect every `@intrinsic("…")` binding in a module's items — top-level
/// functions only. Impl methods and structs are not yet scanned (deferred; see
/// the design doc §5). This differs from `collect_lang_bindings` which also scans
/// impl-associated methods — that difference is intentional until `@intrinsic` on
/// methods has a concrete use case.
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
        let root = <axiom_parser::ast::SourceFile as axiom_parser::ast::AstNode>::cast(result.tree)
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
        let root = <axiom_parser::ast::SourceFile as axiom_parser::ast::AstNode>::cast(result.tree)
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

    /// Drift guard: the raw intrinsic name strings ("heap_alloc", "heap_free",
    /// "heap_get", "heap_set") must not reappear as string literals outside this
    /// module (mirrors `lang.rs`'s `test_no_raw_qualified_list_strings_outside_lang_module`).
    #[test]
    fn test_no_raw_heap_strings_outside_intrinsic_module() {
        let banned = [HEAP_ALLOC, HEAP_FREE, HEAP_GET, HEAP_SET];
        let crate_roots = ["axiom-hir", "axiom-typeck", "axiom-ir", "axiom-vm"];
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("crates/axiom-hir → repo root")
            .to_path_buf();
        let this_file = std::path::Path::new(file!())
            .file_name()
            .map(|f| f.to_os_string());

        let mut offenders: Vec<String> = Vec::new();
        // Allowlist: files that legitimately reference heap intrinsic names.
        let allow: &[&str] = &[
            "builtin-to-stdlib-migration.md",
            "intrinsic-and-stdlib-identity.md",
            "mem.ax",
            "lang-items-and-desugaring-design.md",
        ];

        for root in crate_roots {
            let src = repo.join("crates").join(root).join("src");
            scan_dir(&src, &banned, &this_file, allow, &mut offenders);
        }

        assert!(
            offenders.is_empty(),
            "raw heap intrinsic string(s) found outside intrinsic.rs — use the \
             axiom_hir::intrinsic constants instead:\n{}",
            offenders.join("\n")
        );
    }

    fn scan_dir(
        dir: &std::path::Path,
        banned: &[&str],
        this_file: &Option<std::ffi::OsString>,
        allow: &[&str],
        offenders: &mut Vec<String>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_dir(&path, banned, this_file, allow, offenders);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            if path.file_name().map(|f| f.to_os_string()) == *this_file {
                continue;
            }
            // Skip files in the allowlist by stem.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if allow.contains(&name) {
                    continue;
                }
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (lineno, line) in text.lines().enumerate() {
                for needle in banned {
                    let quoted = format!("\"{needle}\"");
                    if line.contains(&quoted) {
                        offenders.push(format!(
                            "{}:{}: {}",
                            path.display(),
                            lineno + 1,
                            line.trim()
                        ));
                    }
                }
            }
        }
    }
}
