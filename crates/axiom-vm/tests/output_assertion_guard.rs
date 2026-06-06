//! H1 drift guard (`docs/mutable-subscript-design.md` §7).
//!
//! Behavioural end-to-end suites must assert on the program's **real output**
//! (`trace.output()`), never on a substring of the full execution trace
//! (`trace.format()`). The latter is what let a silent indexed-write no-op pass:
//! `out.contains("9")` matched the `Const(Int(9))` that *constructed* the value,
//! independent of whether the write landed (§6).
//!
//! This test fails the build if any `*_e2e.rs` suite calls the trace formatter
//! `t.format()`. Trace-text goldens are legitimate, but they live in dedicated
//! golden/snapshot tests, not in the behavioural `_e2e.rs` suites.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

#[test]
fn e2e_suites_assert_real_output_not_trace_text() {
    let mut offenders = Vec::new();
    for entry in fs::read_dir(tests_dir()).expect("read tests dir") {
        let path = entry.expect("dir entry").path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !name.ends_with("_e2e.rs") {
            continue;
        }
        let src = fs::read_to_string(&path).expect("read e2e source");
        // `t.format()` is the trace formatter. Axiom source strings use the
        // `format("{}", …)` library call (no leading `t.`), so this is
        // unambiguous and will not match program text inside the fixtures.
        if src.contains("t.format()") {
            offenders.push(name);
        }
    }
    assert!(
        offenders.is_empty(),
        "these behavioural e2e suites assert against the full trace text \
         (`t.format()`) instead of real output (`t.output()`): {offenders:?} — \
         see docs/mutable-subscript-design.md §7 H1"
    );
}
