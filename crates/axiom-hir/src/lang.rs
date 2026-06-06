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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_subscript_fn_qualifies() {
        assert_eq!(subscript_fn("List"), "List::subscript");
        assert_eq!(subscript_fn("Grid"), "Grid::subscript");
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
        let crate_roots = ["axiom-hir", "axiom-typeck", "axiom-ir", "axiom-vm"];
        // This file lives at <repo>/crates/axiom-hir/src/lang.rs.
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("crates/axiom-hir → repo root")
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
             axiom_hir::lang constants instead:\n{}",
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
