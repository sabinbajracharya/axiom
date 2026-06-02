//! Golden tree snapshots (`docs/parser-testing.md` §7, Layer 2). Each
//! `fixtures/*.ax` is parsed and serialized; the output is compared to the
//! checked-in `*.ast` golden. Regenerate with `UPDATE_SNAPSHOTS=1`.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use axiom_parser::{parse, serialize};

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
fn golden_tree_snapshots() {
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let dir = fixtures_dir();
    let files = ax_files(&dir);
    assert!(!files.is_empty(), "no .ax fixtures in {}", dir.display());

    for path in files {
        let source = fs::read_to_string(&path).expect("read .ax fixture");
        let result = parse(&source);
        // Happy-path fixtures (everything outside errors/) must parse cleanly —
        // the same discipline the lexer goldens enforce one level down.
        assert!(
            result.errors.is_empty(),
            "fixture {} produced unexpected diagnostics: {:?}",
            path.display(),
            result.errors
        );
        let got = serialize(&result.tree);
        let golden = path.with_extension("ast");

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
            "tree snapshot mismatch for {}",
            path.display()
        );
    }
}
