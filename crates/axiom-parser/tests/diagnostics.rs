//! Diagnostic snapshots (`docs/parser-testing.md` §7, Layer 4). Each
//! `fixtures/errors/*.ax` is parsed; its rendered diagnostics (one `line:col:
//! message` per line) are compared to the checked-in `*.stderr` golden.
//! Regenerate with `UPDATE_SNAPSHOTS=1`.
//!
//! These fixtures are *expected* to produce diagnostics — but, per the coverage
//! invariants test, their trees still reconstruct and tile (recovery never
//! drops source).

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use axiom_parser::parse;

fn errors_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/errors")
}

fn ax_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read errors dir {}: {e}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ax"))
        .collect();
    files.sort();
    files
}

fn render(source: &str) -> String {
    let result = parse(source);
    let mut out = String::new();
    for err in &result.errors {
        out.push_str(&err.render(source));
        out.push('\n');
    }
    out
}

#[test]
fn diagnostic_snapshots() {
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let dir = errors_dir();
    let files = ax_files(&dir);
    assert!(!files.is_empty(), "no error fixtures in {}", dir.display());

    for path in files {
        let source = fs::read_to_string(&path).expect("read error fixture");
        let got = render(&source);
        // Recovery must actually flag something — an error fixture with zero
        // diagnostics is a silent-accept bug.
        assert!(
            !got.is_empty(),
            "error fixture {} produced no diagnostics",
            path.display()
        );
        let golden = path.with_extension("stderr");
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
            got.replace("\r\n", "\n"),
            want.replace("\r\n", "\n"),
            "diagnostic snapshot mismatch for {}",
            path.display()
        );
    }
}
