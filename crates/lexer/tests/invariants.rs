//! Coverage-invariant tests over every fixture (`docs/lexer-testing.md` §4,
//! Layer 3). Every fixture — including the malformed ones under `errors/` —
//! must tile the source, reconstruct exactly, and have spans matching their
//! text. Lexing is total, so this holds even when diagnostics are produced.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use lexer::{check_all, lex};

/// All `*.ax` fixtures, including the `errors/` subdirectory, sorted.
fn all_fixtures() -> Vec<PathBuf> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files = Vec::new();
    collect_ax(&base, &mut files);
    collect_ax(&base.join("errors"), &mut files);
    files.sort();
    files
}

fn collect_ax(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().and_then(|s| s.to_str()) == Some("ax") {
            out.push(path);
        }
    }
}

#[test]
fn every_fixture_satisfies_invariants() {
    let files = all_fixtures();
    assert!(!files.is_empty(), "no fixtures found");
    for path in files {
        let source = fs::read_to_string(&path).expect("read fixture");
        let result = lex(&source);
        if let Err(reason) = check_all(&result.tokens, &source) {
            panic!("invariant failed for {}: {reason}", path.display());
        }
    }
}
