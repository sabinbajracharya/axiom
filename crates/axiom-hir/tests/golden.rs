//! Golden HIR snapshots. Each `fixtures/*.ax` is parsed, lowered, and
//! serialized; the output is compared to the checked-in `.hir` golden.
//! Regenerate with `UPDATE_SNAPSHOTS=1`.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use axiom_hir::{lower, serialize};
use axiom_parser::ast::{AstNode, SourceFile};
use axiom_parser::parse;

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

#[test]
fn golden_hir_snapshots() {
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
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
        let hir = lower(&root, &source);
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
