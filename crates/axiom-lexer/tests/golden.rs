//! Golden snapshot tests (`docs/lexer-testing.md` §2, Layer 2). Each
//! `fixtures/*.ax` is lexed and serialized; the output is compared to the
//! checked-in `*.tokens` golden. Regenerate with `UPDATE_SNAPSHOTS=1`.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use axiom_lexer::{lex, serialize};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Collect `*.ax` files in a directory, sorted for deterministic iteration.
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
fn golden_token_snapshots() {
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let dir = fixtures_dir();
    let files = ax_files(&dir);
    assert!(!files.is_empty(), "no .ax fixtures in {}", dir.display());

    for path in files {
        let source = fs::read_to_string(&path).expect("read .ax fixture");
        let result = lex(&source);
        let got = serialize(&result.tokens, &source);
        let golden = path.with_extension("tokens");

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
            "token snapshot mismatch for {}",
            path.display()
        );
    }
}
