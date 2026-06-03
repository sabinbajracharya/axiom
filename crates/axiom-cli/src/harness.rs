//! The `.ax` feature-test harness: discover the `corpus/**` programs so tests can
//! run every one through the pipeline. The *pattern* is harvested from Oxy's
//! feature-test harness, re-implemented here with no dependencies. As later
//! milestones land (run, build), the same discovered corpus is what their
//! end-to-end + parity suites iterate — so the walker lives in the library, not
//! buried in one test file.
//!
//! The corpus is organized by **expected outcome**, a milestone-stable axis (a
//! program never moves as the pipeline grows):
//! - `corpus/valid/**` — well-formed programs: parse clean now, run with the
//!   expected output at M4.
//! - `corpus/errors/**` — programs that must produce diagnostics (negative tests).

use std::path::{Path, PathBuf};
use std::{fs, io};

/// The workspace-root `corpus/` directory, resolved relative to this crate's
/// manifest so it works regardless of the test's working dir.
pub fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

/// Every `.ax` file under `root`, recursively, sorted for deterministic order.
pub fn discover(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    collect(root, &mut found)?;
    found.sort();
    Ok(found)
}

/// Does this corpus path live under an `errors/` directory (a negative test that
/// must produce diagnostics), as opposed to a `valid/` program?
pub fn expects_errors(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "errors")
}

/// Depth-first walk pushing every `*.ax` file path into `out`.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "ax") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // Unit tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn test_corpus_dir_exists() {
        let dir = corpus_dir();
        assert!(dir.is_dir(), "corpus dir missing: {}", dir.display());
    }

    #[test]
    fn test_discover_finds_seed_corpus() {
        let files = discover(&corpus_dir()).expect("read corpus dir");
        assert!(!files.is_empty(), "no .ax files discovered");
        assert!(files
            .iter()
            .all(|p| p.extension().is_some_and(|e| e == "ax")));
        // Sorted implies the walk is deterministic.
        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(files, sorted);
    }

    #[test]
    fn test_expects_errors_classifies_by_directory() {
        assert!(expects_errors(Path::new("corpus/errors/missing_expr.ax")));
        assert!(!expects_errors(Path::new("corpus/valid/hello.ax")));
    }
}
