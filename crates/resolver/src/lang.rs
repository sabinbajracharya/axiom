//! Well-known stdlib names the compiler is welded to — the single source of
//! truth for the compiler→stdlib coupling described in
//! [`docs/lang-items-and-desugaring-design.md`](../../../docs/lang-items-and-desugaring-design.md)
//! §3.1.
//!
//! A handful of language constructs are *defined* in terms of specific stdlib
//! items: `[a, b, c]` builds a `List`, `base[i]` calls a `subscript`. Both the
//! type checker and IR lowering need to name those items. Spelling them inline
//! in every stage was the original coupling smell (the same name re-typed in
//! several crates, drifting silently if the stdlib renamed a method). Every such
//! spelling now lives here, as a named constant, so the coupling is **one
//! greppable place** — the `symbols.rs` discipline the lexer follows
//! (`docs/lexer-testing.md` §5.2), applied to stdlib names.
//!
//! The drift guard in `lang::tests` (mirrored from the lexer's source-scan test)
//! fails the build if a qualified `List::…` method string reappears as a raw
//! literal outside this module.

/// The growable-list type backing list literals (`[a, b, c]`).
pub const LIST: &str = "List";

/// The associated method name used for indexing (`base[i]` → `Type::subscript`).
pub const SUBSCRIPT: &str = "subscript";

/// The associated method name used for indexed-place *writes*
/// (`base[i] = v` → `Type::subscript_set`). The setter counterpart of
/// [`SUBSCRIPT`] (`docs/mutable-subscript-design.md` §4.2).
pub const SUBSCRIPT_SET: &str = "subscript_set";

/// `List::new()` — an empty list (used by empty / unsized literals).
pub const LIST_NEW: &str = "List::new";

/// `List::with_capacity(n)` — a list pre-sized to `n` (used by sized literals so
/// `[a, b, c]` allocates exactly once instead of regrowing).
pub const LIST_WITH_CAPACITY: &str = "List::with_capacity";

/// `List::push(value)` — append one element (the per-element step of a literal).
pub const LIST_PUSH: &str = "List::push";

/// Build the qualified subscript function name for a receiver type, e.g.
/// `subscript_fn("List")` → `"List::subscript"`. Mirrors the lowering of a
/// `subscript` declaration to a `Type::subscript(self, index…)` function.
pub fn subscript_fn(type_name: &str) -> String {
    format!("{type_name}::{SUBSCRIPT}")
}

/// Build the qualified *setter* subscript function name for a receiver type,
/// e.g. `subscript_set_fn("List")` → `"List::subscript_set"`. The write
/// counterpart of [`subscript_fn`]: `base[i] = v` dispatches to this function
/// (`docs/mutable-subscript-design.md` §4.2).
pub fn subscript_set_fn(type_name: &str) -> String {
    format!("{type_name}::{SUBSCRIPT_SET}")
}

// ── Lang items ────────────────────────────────────────────────────────────────
//
// A lang item is a stdlib definition the compiler is welded to, bound by a
// `@lang("key")` attribute rather than matched by spelling
// (`docs/lang-items-and-desugaring-design.md` §3.3). The registry below resolves
// every required key to the **real** stdlib `DefId`, killing the placeholder
// `HirId(0)` that list-literal typing used to fabricate (§3.2).

use crate::error::HirDiagnostic;
use crate::hir_types::{HirId, Item};
use lexer::Span;

/// The `@lang("…")` key for the list type backing `[a, b, c]`.
pub const LANG_LIST: &str = "list";
/// The `@lang("…")` key for `List::new`.
pub const LANG_LIST_NEW: &str = "list_new";
/// The `@lang("…")` key for `List::with_capacity`.
pub const LANG_LIST_WITH_CAPACITY: &str = "list_with_capacity";
/// The `@lang("…")` key for `List::push`.
pub const LANG_LIST_PUSH: &str = "list_push";

/// Every lang-item key the compiler requires the stdlib to bind exactly once.
/// Adding a key here without a stdlib `@lang` binding fails the build (the
/// registry consistency guarantee, §6.2) — the lang-item analogue of an unnamed
/// `TokenKind` failing the lexer's symbol-consistency test.
pub const REQUIRED_LANG_ITEMS: &[&str] = &[
    LANG_LIST,
    LANG_LIST_NEW,
    LANG_LIST_WITH_CAPACITY,
    LANG_LIST_PUSH,
];

/// The compiler-required lang items, resolved to their real stdlib `DefId`s.
/// Populated once after name resolution; read by typeck (and, later, the HIR
/// desugar pass) so synthesized list-literal types/calls point at the true
/// `List` definition instead of a placeholder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LangItems {
    pub list: Option<HirId>,
    pub list_new: Option<HirId>,
    pub list_with_capacity: Option<HirId>,
    pub list_push: Option<HirId>,
}

impl LangItems {
    /// Look up a resolved lang item by key.
    fn get(&self, key: &str) -> Option<HirId> {
        match key {
            LANG_LIST => self.list,
            LANG_LIST_NEW => self.list_new,
            LANG_LIST_WITH_CAPACITY => self.list_with_capacity,
            LANG_LIST_PUSH => self.list_push,
            _ => None,
        }
    }

    /// Record a lang item, returning `false` if the key was already bound
    /// (a duplicate) or is not a recognized key (an orphan tag).
    fn set(&mut self, key: &str, def_id: HirId) -> SetOutcome {
        let slot = match key {
            LANG_LIST => &mut self.list,
            LANG_LIST_NEW => &mut self.list_new,
            LANG_LIST_WITH_CAPACITY => &mut self.list_with_capacity,
            LANG_LIST_PUSH => &mut self.list_push,
            _ => return SetOutcome::Orphan,
        };
        if slot.is_some() {
            return SetOutcome::Duplicate;
        }
        *slot = Some(def_id);
        SetOutcome::Bound
    }
}

enum SetOutcome {
    Bound,
    Duplicate,
    Orphan,
}

/// A `@lang("key")` binding discovered in a module's HIR: the key and the real
/// `DefId` it annotates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LangBinding {
    pub key: String,
    pub def_id: HirId,
}

/// Collect every `@lang("…")` binding in a module's items — top-level structs
/// and functions, plus impl-associated methods (where `List::new`/`push`/… live,
/// so a `Def`-only scan would miss them).
pub fn collect_lang_bindings(items: &[Item]) -> Vec<LangBinding> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::StructDef(s) => push_tag(&mut out, &s.lang_tag, s.id),
            Item::FnDef(f) => push_tag(&mut out, &f.lang_tag, f.id),
            Item::ImplDef(i) => {
                for m in &i.methods {
                    push_tag(&mut out, &m.lang_tag, m.id);
                }
            }
            _ => {}
        }
    }
    out
}

fn push_tag(out: &mut Vec<LangBinding>, tag: &Option<String>, def_id: HirId) {
    if let Some(key) = tag {
        out.push(LangBinding {
            key: key.clone(),
            def_id,
        });
    }
}

/// Assemble the lang-item registry from the bindings found in the **stdlib**.
/// Produces a consistency diagnostic for every drift (§6.2/§6.5):
///
/// - a duplicate binding for one key → [`HirDiagnostic::DuplicateLangItem`];
/// - a `@lang` tag with no recognized key → [`HirDiagnostic::OrphanLangItem`];
/// - (when `enforce_required`) a required key with no binding →
///   [`HirDiagnostic::MissingLangItem`].
///
/// `enforce_required` is `true` exactly when the stdlib is loaded; the bare
/// no-stdlib test mode deliberately has no lang items and skips the requirement.
pub fn resolve_lang_items(
    stdlib_bindings: &[LangBinding],
    enforce_required: bool,
) -> (LangItems, Vec<HirDiagnostic>) {
    let mut items = LangItems::default();
    let mut diags = Vec::new();
    for binding in stdlib_bindings {
        match items.set(&binding.key, binding.def_id) {
            SetOutcome::Bound => {}
            SetOutcome::Duplicate => diags.push(HirDiagnostic::DuplicateLangItem {
                key: binding.key.clone(),
                span: Span { lo: 0, hi: 0 },
            }),
            SetOutcome::Orphan => diags.push(HirDiagnostic::OrphanLangItem {
                key: binding.key.clone(),
                span: Span { lo: 0, hi: 0 },
            }),
        }
    }
    if enforce_required {
        for key in REQUIRED_LANG_ITEMS {
            if items.get(key).is_none() {
                diags.push(HirDiagnostic::MissingLangItem {
                    key: (*key).to_string(),
                    span: Span { lo: 0, hi: 0 },
                });
            }
        }
    }
    (items, diags)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_subscript_fn_qualifies() {
        assert_eq!(subscript_fn("List"), "List::subscript");
        assert_eq!(subscript_fn("Grid"), "Grid::subscript");
    }

    fn binding(key: &str, id: usize) -> LangBinding {
        LangBinding {
            key: key.to_string(),
            def_id: HirId(id),
        }
    }

    fn full_bindings() -> Vec<LangBinding> {
        vec![
            binding(LANG_LIST, 1),
            binding(LANG_LIST_NEW, 2),
            binding(LANG_LIST_WITH_CAPACITY, 3),
            binding(LANG_LIST_PUSH, 4),
        ]
    }

    #[test]
    fn test_resolve_lang_items_complete_set_is_clean() {
        let (items, diags) = resolve_lang_items(&full_bindings(), true);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        assert_eq!(items.list, Some(HirId(1)));
        assert_eq!(items.list_new, Some(HirId(2)));
        assert_eq!(items.list_with_capacity, Some(HirId(3)));
        assert_eq!(items.list_push, Some(HirId(4)));
    }

    #[test]
    fn test_resolve_lang_items_missing_required_emits_diagnostic() {
        // Drop `list_push`; the requirement check should flag exactly it.
        let mut bindings = full_bindings();
        bindings.pop();
        let (_items, diags) = resolve_lang_items(&bindings, true);
        assert_eq!(diags.len(), 1, "diags: {diags:?}");
        assert!(matches!(
            &diags[0],
            HirDiagnostic::MissingLangItem { key, .. } if key == LANG_LIST_PUSH
        ));
    }

    #[test]
    fn test_resolve_lang_items_no_enforcement_skips_missing() {
        // The no-stdlib mode: an empty binding set is fine when not enforcing.
        let (items, diags) = resolve_lang_items(&[], false);
        assert!(diags.is_empty(), "diags: {diags:?}");
        assert_eq!(items, LangItems::default());
    }

    #[test]
    fn test_resolve_lang_items_duplicate_emits_diagnostic() {
        let mut bindings = full_bindings();
        bindings.push(binding(LANG_LIST, 99));
        let (_items, diags) = resolve_lang_items(&bindings, true);
        assert!(
            diags.iter().any(
                |d| matches!(d, HirDiagnostic::DuplicateLangItem { key, .. } if key == LANG_LIST)
            ),
            "expected DuplicateLangItem, got {diags:?}"
        );
    }

    #[test]
    fn test_resolve_lang_items_orphan_tag_emits_diagnostic() {
        let mut bindings = full_bindings();
        bindings.push(binding("not_a_lang_item", 50));
        let (_items, diags) = resolve_lang_items(&bindings, true);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                HirDiagnostic::OrphanLangItem { key, .. } if key == "not_a_lang_item"
            )),
            "expected OrphanLangItem, got {diags:?}"
        );
    }

    #[test]
    fn test_collect_lang_bindings_from_struct_and_impl_methods() {
        // `@lang` on a struct and on impl-associated methods are all collected,
        // including the methods that a `Def`-only scan would miss.
        let source = "\
@lang(\"list\")
struct List<T> { count: Int }
impl<T> List<T> {
    @lang(\"list_new\")
    fn new() -> List<T> { List { count: 0 } }
    fn untagged() -> Int { 0 }
}
";
        let result = parser::parse(source);
        let root = <parser::ast::SourceFile as parser::ast::AstNode>::cast(result.tree).unwrap();
        let (items, _defs, _diags, _nid) = crate::lower_structural(&root, source, 0);
        let bindings = collect_lang_bindings(&items);
        let keys: Vec<&str> = bindings.iter().map(|b| b.key.as_str()).collect();
        assert!(keys.contains(&LANG_LIST), "keys: {keys:?}");
        assert!(keys.contains(&LANG_LIST_NEW), "keys: {keys:?}");
        assert_eq!(
            bindings.len(),
            2,
            "untagged methods must not appear: {keys:?}"
        );
    }

    /// Drift guard (`docs/lang-items-and-desugaring-design.md` §6.2, "no raw
    /// stdlib-name strings outside the names module"): the qualified `List::…`
    /// method strings must not reappear as raw literals anywhere in the compiler
    /// source outside this module. Adding one back fails the build, the way an
    /// unnamed `TokenKind` fails the lexer's symbol-consistency test.
    ///
    /// Narrow by design: only the *qualified* method spellings are banned (the
    /// genuinely magic ones). The bare type name `"List"` legitimately appears in
    /// many comparison/diagnostic contexts and is not scanned.
    #[test]
    fn test_no_raw_qualified_list_strings_outside_lang_module() {
        let banned = [LIST_NEW, LIST_WITH_CAPACITY, LIST_PUSH];
        let crate_roots = ["hir", "typecheck", "ir", "vm"];
        // This file lives at <repo>/crates/axiom-hir/src/lang.rs.
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("crates/resolver → repo root")
            .to_path_buf();
        let this_file = std::path::Path::new(file!())
            .file_name()
            .map(|f| f.to_os_string());

        let mut offenders: Vec<String> = Vec::new();
        for root in crate_roots {
            let src = repo.join("crates").join(root).join("src");
            scan_dir(&src, &banned, &this_file, &mut offenders);
        }
        assert!(
            offenders.is_empty(),
            "raw qualified List::… string(s) found outside lang.rs — use the \
             hir::lang constants instead:\n{}",
            offenders.join("\n")
        );
    }

    fn scan_dir(
        dir: &std::path::Path,
        banned: &[&str],
        this_file: &Option<std::ffi::OsString>,
        offenders: &mut Vec<String>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_dir(&path, banned, this_file, offenders);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            if path.file_name().map(|f| f.to_os_string()) == *this_file {
                continue; // this module is the one allowed home for the strings
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
