//! Diagnostic snapshot tests (`docs/lexer-testing.md` §1, Layer 4). Each
//! malformed `fixtures/errors/*.ax` is lexed; the rendered diagnostics (message,
//! span, and line:col) are pinned to a `*.stderr` golden so error reporting
//! can't silently regress. Regenerate with `UPDATE_SNAPSHOTS=1`.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use axiom_lexer::{lex, LineMap};

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

/// Render diagnostics deterministically, one per line.
fn render(source: &str) -> String {
    let result = lex(source);
    let lines = LineMap::new(source);
    let mut out = String::new();
    if result.errors.is_empty() {
        return "(no diagnostics)\n".to_string();
    }
    for err in &result.errors {
        let span = err.span();
        let (l1, c1) = lines.locate(source, span.lo);
        let (l2, c2) = lines.locate(source, span.hi);
        let _ = writeln!(out, "{l1}:{c1}-{l2}:{c2} ({}..{}): {err}", span.lo, span.hi);
    }
    out
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[test]
fn diagnostic_snapshots() {
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let files = ax_files(&errors_dir());
    assert!(!files.is_empty(), "no error fixtures found");

    for path in files {
        let source = fs::read_to_string(&path).expect("read error fixture");
        let got = render(&source);
        let golden = path.with_extension("stderr");

        if update {
            fs::write(&golden, &got).expect("write stderr golden");
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
            "diagnostic mismatch for {}",
            path.display()
        );
    }
}
