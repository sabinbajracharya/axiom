//! Golden HIR snapshots for multi-file programs. Each `fixtures/modules/*/`
//! directory is a test case containing multiple `.ax` files. The two-phase
//! compilation pipeline runs: structural lowering → build global exports →
//! resolve with globals → combine HIRs.
//!
//! The combined HIR is serialized and compared to a `main.hir` golden file
//! in the test case directory. Regenerate with `UPDATE_SNAPSHOTS=1`.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use resolver::{
    build_global_exports, resolve_with_globals,
};
use lower::{
    lower_structural, serialize, Hir, HirDiagnostic, Item,
};
use parser::ast::AstNode;
use parser::parse;

fn modules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modules")
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Load stdlib module defs for cross-module resolution.
fn load_stdlib_defs() -> Option<Vec<(String, Vec<lower::Def>)>> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent()?.parent()?;
    let stdlib = workspace.join("stdlib");
    if !stdlib.exists() {
        return None;
    }
    let graph = modules::discover::discover_library(&stdlib).ok()?;
    let mut result = Vec::new();
    let mut topo = graph.topo_order();
    topo.sort_by_key(|id| graph.get(*id).name.clone());
    for module_id in topo {
        let module = graph.get(module_id);
        if module.source.is_empty() {
            continue;
        }
        let parse_result = parse(&module.source);
        let Some(root) = parser::ast::SourceFile::cast(parse_result.tree) else {
            continue;
        };
        let (_items, defs, _diags, _nid) = lower_structural(&root, &module.source, 0);
        result.push((module.name.clone(), defs));
    }
    Some(result)
}

/// Load stdlib module items for merging into the test module graph.
fn load_stdlib_modules() -> Option<Vec<ModuleData>> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent()?.parent()?;
    let stdlib = workspace.join("stdlib");
    if !stdlib.exists() {
        return None;
    }
    let graph = modules::discover::discover_library(&stdlib).ok()?;
    let mut result = Vec::new();
    let mut topo = graph.topo_order();
    topo.sort_by_key(|id| graph.get(*id).name.clone());
    for module_id in topo {
        let module = graph.get(module_id);
        if module.source.is_empty() {
            continue;
        }
        let parse_result = parse(&module.source);
        let Some(root) = parser::ast::SourceFile::cast(parse_result.tree) else {
            continue;
        };
        let (items, defs, diags, _nid) = lower_structural(&root, &module.source, 0);
        result.push((module.name.clone(), items, defs, diags));
    }
    Some(result)
}

/// Discover all `.ax` files in a directory, sorted by name.
fn ax_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read dir {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ax"))
        .collect();
    files.sort();
    files
}

type ModuleData = (String, Vec<Item>, Vec<lower::Def>, Vec<HirDiagnostic>);

/// Compile a multi-file program using the two-phase pipeline.
/// Returns (combined_hir, diagnostics_by_module).
fn compile_multi_file(dir: &Path) -> (Hir, HashMap<String, Vec<HirDiagnostic>>) {
    let files = ax_files_in(dir);
    assert!(!files.is_empty(), "no .ax files in {}", dir.display());

    // Phase 1: structural lowering with globally unique IDs.
    let mut all_module_data: Vec<ModuleData> = Vec::new();
    let mut next_id: usize = 1;

    // Include stdlib modules so io::print/println are available.
    if let Some(stdlib_modules) = load_stdlib_modules() {
        for (name, items, defs, diags) in stdlib_modules {
            let max_id = defs.iter().map(|d| d.def_id.0).max().unwrap_or(0);
            next_id = next_id.max(max_id + 1);
            all_module_data.push((name, items, defs, diags));
        }
    }

    for path in &files {
        let source = fs::read_to_string(path).expect("read .ax");
        let result = parse(&source);
        assert!(
            result.errors.is_empty(),
            "parse errors in {}: {:?}",
            path.display(),
            result.errors
        );
        let root = parser::ast::SourceFile::cast(result.tree).unwrap();
        let module_name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let (items, defs, diagnostics, nid) = lower_structural(&root, &source, next_id);
        next_id = nid;
        all_module_data.push((module_name, items, defs, diagnostics));
    }

    // Phase 2: build global exports from all modules + stdlib.
    let mut module_defs: Vec<(String, Vec<lower::Def>)> = all_module_data
        .iter()
        .map(|(name, _, defs, _)| (name.clone(), defs.clone()))
        .collect();
    // Include stdlib modules so io::print/println resolve.
    if let Some(stdlib_data) = load_stdlib_defs() {
        module_defs.extend(stdlib_data);
    }
    let global_exports = build_global_exports(&module_defs);

    // Phase 3: resolve each module with cross-module context.
    let mut all_items = Vec::new();
    let mut all_diagnostics = HashMap::new();

    for (module_name, mut items, defs, structural_diags) in all_module_data {
        let mut diags = structural_diags;
        resolve_with_globals(&mut items, &defs, &mut diags, &global_exports, &module_name);
        all_items.extend(items);
        if !diags.is_empty() {
            all_diagnostics.insert(module_name, diags);
        }
    }

    let hir = Hir {
        items: all_items,
        diagnostics: Vec::new(), // diagnostics tracked separately by module
    };
    (hir, all_diagnostics)
}

/// Render diagnostics for golden file comparison.
fn render_diagnostics(
    diags: &HashMap<String, Vec<HirDiagnostic>>,
    sources: &HashMap<String, String>,
) -> String {
    let mut out = String::new();
    let mut modules: Vec<_> = diags.keys().collect();
    modules.sort();
    for module in modules {
        let module_diags = &diags[module];
        let source = sources.get(module).map(|s| s.as_str()).unwrap_or("");
        for diag in module_diags {
            out.push_str(&HirDiagnostic::render(diag, source));
            out.push('\n');
        }
    }
    out
}

#[test]
fn golden_multi_file_hir() {
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let dir = modules_dir();
    if !dir.exists() {
        // No multi-file test cases yet.
        return;
    }

    let mut test_cases: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    test_cases.sort();

    for case_dir in test_cases {
        let case_name = case_dir.file_name().unwrap().to_str().unwrap();

        // Skip error test cases (handled by golden_multi_file_diagnostics).
        if case_name.contains("error") {
            continue;
        }

        let (hir, diag_map) = compile_multi_file(&case_dir);
        assert!(
            diag_map.is_empty(),
            "fixture {} produced unexpected diagnostics: {:?}",
            case_name,
            diag_map
        );

        let got = serialize(&hir);
        let golden = case_dir.join("main.hir");

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
            "HIR snapshot mismatch for multi-file case {case_name}"
        );
    }
}

#[test]
fn golden_multi_file_diagnostics() {
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let dir = modules_dir();
    if !dir.exists() {
        return;
    }

    let mut test_cases: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    test_cases.sort();

    for case_dir in test_cases {
        let case_name = case_dir.file_name().unwrap().to_str().unwrap();

        // Only run error test cases.
        if !case_name.contains("error") {
            continue;
        }

        // Load sources for diagnostic rendering.
        let mut sources = HashMap::new();
        for path in ax_files_in(&case_dir) {
            let module_name = path.file_stem().unwrap().to_str().unwrap().to_string();
            let source = fs::read_to_string(&path).expect("read .ax");
            sources.insert(module_name, source);
        }

        let (_hir, diag_map) = compile_multi_file(&case_dir);
        assert!(
            !diag_map.is_empty(),
            "error fixture {case_name} produced no diagnostics"
        );

        let got = render_diagnostics(&diag_map, &sources);
        let golden = case_dir.join("main.stderr");

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
            "diagnostic snapshot mismatch for multi-file case {case_name}"
        );
    }
}
