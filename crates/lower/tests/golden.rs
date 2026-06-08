//! Golden HIR snapshots. Each `fixtures/*.ax` is parsed, lowered, and
//! serialized; the output is compared to the checked-in `.hir` golden.
//! Regenerate with `UPDATE_SNAPSHOTS=1`.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use resolver::lower;
use lower::serialize;
use parser::ast::{AstNode, SourceFile};
use parser::parse;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ax_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read fixtures dir {}: {e}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ax"))
        .collect();
    files.sort();
    files
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Build global exports from stdlib modules so `print`/`println` resolve.
fn stdlib_exports() -> Option<resolver::GlobalExports> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent()?.parent()?;
    let stdlib = workspace.join("stdlib");
    if !stdlib.exists() {
        return None;
    }
    let graph = modules::discover::discover_library(&stdlib).ok()?;
    let mut module_data = Vec::new();
    for module_id in graph.topo_order() {
        let module = graph.get(module_id);
        if module.source.is_empty() {
            continue;
        }
        let parse_result = parser::parse(&module.source);
        let Some(root) = parser::ast::SourceFile::cast(parse_result.tree) else {
            continue;
        };
        let (_items, defs, _diags, _nid) = lower::lower_structural(&root, &module.source, 0);
        module_data.push((module.name.clone(), defs));
    }
    Some(resolver::build_global_exports(&module_data))
}

#[test]
fn golden_hir_snapshots() {
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let exports = stdlib_exports();
    let dir = fixtures_dir();
    let files = ax_files(&dir);
    assert!(!files.is_empty(), "no .ax fixtures in {}", dir.display());

    for path in files {
        let source = fs::read_to_string(&path).expect("read .ax fixture");
        let result = parse(&source);
        assert!(
            result.errors.is_empty(),
            "fixture {} produced unexpected parse errors: {:?}",
            path.display(),
            result.errors
        );
        let root = SourceFile::cast(result.tree).unwrap();
        let hir = lower(&root, &source, exports.as_ref());
        assert!(
            hir.diagnostics.is_empty(),
            "fixture {} produced unexpected HIR diagnostics: {:?}",
            path.display(),
            hir.diagnostics
        );
        let got = serialize(&hir);
        let golden = path.with_extension("hir");

        if update {
            fs::write(&golden, &got).expect("write golden");
            continue;
        }
        let want = fs::read_to_string(&golden).unwrap_or_else(|_| {
            panic!(
                "missing golden {} — run UPDATE_SNAPSHOTS=1",
                golden.display()
            )
        });
        assert_eq!(
            normalize(&got),
            normalize(&want),
            "HIR snapshot mismatch for {}",
            path.display()
        );
    }
}
