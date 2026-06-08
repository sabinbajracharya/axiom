//! Coverage invariants over every fixture (`docs/parser-testing.md` §4, Layer
//! 3). For each `.ax` (happy-path *and* error fixtures), the parsed tree must
//! reconstruct the source, tile it, and contain every significant lexer token.
//! This is the load-bearing "nothing was missed" check.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use parser::{check_all, parse};

fn all_ax_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut out = Vec::new();
    collect(&root, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("ax") {
            out.push(path);
        }
    }
}

#[test]
fn every_fixture_satisfies_coverage_invariants() {
    let files = all_ax_files();
    assert!(!files.is_empty(), "no fixtures found");
    for path in files {
        let source = fs::read_to_string(&path).expect("read fixture");
        let result = parse(&source);
        let tokens = lexer::lex(&source).tokens;
        if let Err(reason) = check_all(&result.tree, &source, &tokens) {
            panic!("invariant failed for {}: {reason}", path.display());
        }
    }
}
