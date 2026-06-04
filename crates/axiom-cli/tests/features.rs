//! M2 feature-test harness, end to end. Every program in `corpus/**` is run
//! through `check` (lex + parse + HIR lower + name resolution + type checking)
//! and asserted against its expected outcome.
//!
//! - `corpus/valid/**` must produce zero diagnostics (parse, HIR, and type).
//!   As M2 adds type checking, valid corpus files must also type-check cleanly.
//! - `corpus/errors/**` must produce at least one diagnostic (negative tests).

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_cli::{check_source, harness};

#[test]
fn test_corpus_is_non_empty() {
    let files = harness::discover(&harness::corpus_dir()).expect("read corpus dir");
    assert!(!files.is_empty(), "no .ax files found under corpus/");
}

#[test]
fn test_every_corpus_file_matches_expected_outcome() {
    let files = harness::discover(&harness::corpus_dir()).expect("read corpus dir");
    for path in files {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let report = check_source(&source);
        // A tree is always produced — parsing is total — and it's a well-formed
        // root (the dump leads with the `SourceFile` node and its byte span).
        assert!(
            report.tree_dump.starts_with("SourceFile @"),
            "{}: did not produce a well-formed SourceFile root, got:\n{}",
            path.display(),
            report.tree_dump.lines().next().unwrap_or("<empty>")
        );
        if harness::expects_errors(&path) {
            assert!(
                !report.is_clean(),
                "{}: expected diagnostics (it lives under errors/) but check was clean",
                path.display()
            );
        } else {
            // At M2, valid corpus files must have zero diagnostics overall
            // (parse, HIR, or type errors).
            assert!(
                report.is_clean(),
                "{}: unexpected diagnostics:\n{}",
                path.display(),
                report
                    .diagnostics
                    .iter()
                    .map(|d| d.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
}

#[test]
fn test_seed_corpus_present() {
    let files = harness::discover(&harness::corpus_dir()).expect("read corpus dir");
    let names: Vec<String> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    for expected in [
        "hello.ax",
        "arithmetic.ax",
        "structs_enums_match.ax",
        "missing_expr.ax",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "seed file {expected} missing from corpus; found {names:?}"
        );
    }
}
