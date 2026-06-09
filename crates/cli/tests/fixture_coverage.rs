//! Cross-pipeline fixture coverage invariant.
//!
//! Every feature that has a fixture in ANY stage must have a fixture in ALL
//! stages. This prevents the common mistake: adding generics to the parser
//! fixtures but forgetting the IR fixtures.
//!
//! Stages are auto-discovered from `crates/axiom-*/tests/fixtures/` dirs.
//! The corpus (`corpus/valid/`) is always included as a stage.
//! Features are discovered from file stems across all stages.
//!
//! Set `FIXTURE_COVERAGE_EXCLUDE` env var to a comma-separated list of
//! `feature:stage` pairs to exclude (e.g. `bom:lexer,unicode:lexer`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

// ── Stage discovery ─────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A pipeline stage: a directory containing fixture files for one compiler layer.
struct Stage {
    name: String,
    dir: PathBuf,
}

/// Auto-discover stages from `crates/axiom-*/tests/fixtures/` plus corpus.
fn discover_stages() -> Vec<Stage> {
    let root = workspace_root();
    let crates_dir = root.join("crates");
    let mut stages = Vec::new();

    // Discover crate fixture dirs.
    if let Ok(entries) = fs::read_dir(&crates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let crate_name = path.file_name().unwrap().to_str().unwrap().to_string();
            // Layer name from crate directory (e.g. "lexer", "typecheck", "vm").
            let layer = crate_name;
            let fixtures_dir = path.join("tests/fixtures");
            if fixtures_dir.is_dir() {
                stages.push(Stage {
                    name: layer,
                    dir: fixtures_dir,
                });
            }
        }
    }

    // Always include corpus.
    stages.push(Stage {
        name: "corpus".to_string(),
        dir: root.join("corpus/valid"),
    });

    // Sort for deterministic order.
    stages.sort_by(|a, b| a.name.cmp(&b.name));
    stages
}

// ── Discovery helpers ───────────────────────────────────────────────────────

/// Collect file stems from a directory (any extension, files only, no subdirs).
fn collect_stems(dir: &Path) -> BTreeSet<String> {
    let mut stems = BTreeSet::new();
    if !dir.is_dir() {
        return stems;
    }
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                stems.insert(stem.to_string());
            }
        }
    }
    stems
}

// ── Exclusion list ──────────────────────────────────────────────────────────

/// Features that are intentionally limited to specific stages.
/// These are stage-specific tests (lexer behavior, parser syntax, typeck
/// scenarios) that don't represent full language features.
///
/// Each entry excludes a feature from all stages except its home stage(s).
fn default_exclusions() -> BTreeSet<(String, String)> {
    let mut set = BTreeSet::new();
    let all_stages = [
        "lexer",
        "parser",
        "lower",
        "typecheck",
        "ir",
        "vm",
        "corpus",
    ];

    // Helper: exclude feature from all stages except those in `home`.
    let mut only_in = |feature: &str, home: &[&str]| {
        for stage in &all_stages {
            if !home.contains(stage) {
                set.insert((feature.to_string(), stage.to_string()));
            }
        }
    };

    // ── Lexer-only ──
    for feature in [
        "bom",
        "unicode",
        "struct_fn",
        "numbers",
        "operators",
        "strings",
    ] {
        only_in(feature, &["lexer"]);
    }

    // ── Parser-only ──
    for feature in [
        "attributes",
        "closures_scope",
        "comments",
        "error_handling",
        "expressions",
        "labels",
        "modules_use",
        "nested_generics",
        "option_try",
    ] {
        only_in(feature, &["parser"]);
    }

    // ── Typeck-only (scenario tests, not full features) ──
    for feature in [
        "break_no_value",
        "break_value",
        "continue",
        "simple_enum",
        "simple_struct",
        "type_mismatch",
    ] {
        only_in(feature, &["typecheck"]);
    }

    // ── HIR + Typeck only ──
    only_in("struct_field_access", &["lower", "typecheck"]);
    only_in("struct_literal", &["lower"]);
    only_in("trait_supertrait", &["parser", "lower"]);

    // ── Error handling (parser/lower only, desugared before later stages) ──
    only_in("else_basic", &["parser"]);
    only_in("error_set_union", &["parser"]);
    only_in("error_set_basic", &["lower"]);
    only_in("error_handling_basic", &["lower"]);

    // ── IR + VM only (integer literal matching, no per-layer .ax fixtures) ──
    for feature in ["int_match", "multi_fn"] {
        only_in(feature, &["ir", "vm"]);
    }

    set
}

/// Merge default exclusions with `FIXTURE_COVERAGE_EXCLUDE` env var.
fn exclusion_set() -> BTreeSet<(String, String)> {
    let mut set = default_exclusions();
    if let Ok(val) = std::env::var("FIXTURE_COVERAGE_EXCLUDE") {
        for pair in val.split(',') {
            let pair = pair.trim();
            if let Some((feature, stage)) = pair.split_once(':') {
                set.insert((feature.trim().to_string(), stage.trim().to_string()));
            }
        }
    }
    set
}

// ── The invariant test ──────────────────────────────────────────────────────

#[test]
fn test_fixture_coverage() {
    let stages = discover_stages();
    let exclusions = exclusion_set();

    // Discover features: union of all stems across all stages.
    let mut all_features: BTreeSet<String> = BTreeSet::new();
    let mut stage_features: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();

    for stage in &stages {
        let stems = collect_stems(&stage.dir);
        all_features.extend(stems.iter().cloned());
        stage_features.insert(&stage.name, stems);
    }

    if all_features.is_empty() {
        panic!("no features discovered across any stage — something is wrong");
    }

    // Check: every feature must have a fixture in every stage (minus exclusions).
    let mut missing: Vec<String> = Vec::new();

    for feature in &all_features {
        for stage in &stages {
            if exclusions.contains(&(feature.clone(), stage.name.clone())) {
                continue;
            }
            let stems = stage_features.get(stage.name.as_str()).unwrap();
            if !stems.contains(feature) {
                missing.push(format!(
                    "  {:<20} missing from {:<10} ({})",
                    feature,
                    stage.name,
                    stage.dir.display(),
                ));
            }
        }
    }

    if !missing.is_empty() {
        panic!(
            "fixture coverage gaps (set FIXTURE_COVERAGE_EXCLUDE to exempt):\n{}",
            missing.join("\n")
        );
    }
}

// ── Sanity: stages are non-empty ────────────────────────────────────────────

#[test]
fn test_all_stages_have_fixtures() {
    let stages = discover_stages();
    let mut empty: Vec<String> = Vec::new();

    for stage in &stages {
        let stems = collect_stems(&stage.dir);
        if stems.is_empty() {
            empty.push(stage.name.clone());
        }
    }

    assert!(
        empty.is_empty(),
        "stage(s) with zero fixtures: {} — every stage must have at least one fixture",
        empty.join(", ")
    );
}
