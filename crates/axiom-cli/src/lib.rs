//! The Axiom compiler driver (`axiom`). Owns the user-facing command surface
//! and the `.ax` feature-test harness.
//!
//! At **M2** the `check` command runs lex + parse + HIR lowering + name
//! resolution + type checking, printing CST, HIR, and THIR dumps. Type
//! errors appear as `TypeDiagnostic`s alongside HIR diagnostics.
//!
//! ```
//! use axiom_cli::check_source;
//! let report = check_source("fn main() { val x = 1 + 2 }");
//! assert!(report.is_clean());
//! ```

mod check;
pub mod cli;
pub mod harness;

pub use check::{check_source, compile_source, CheckReport, CompileResult};
pub use cli::{parse_args, CliError, Command};

use axiom_parser::ast::AstNode;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Exit code when the source had diagnostics (a *clean* failure, not a crash).
const EXIT_DIAGNOSTICS: u8 = 1;
/// Exit code for a usage mistake (bad args) or an I/O failure reading the file.
const EXIT_USAGE: u8 = 2;
/// Exit code for a recognized-but-unimplemented command (`run` / `build`).
const EXIT_UNIMPLEMENTED: u8 = 3;

const HELP: &str = "\
axiom — the Axiom compiler driver

USAGE:
    axiom <command> <path>

COMMANDS:
    check <path>    Lex, parse, and type-check; report diagnostics
    run <path>      Execute a program via the register-IR interpreter
    build <path>    Build a native executable (not yet implemented)
    help            Show this help
    version         Show the version

<path> may be a single .ax file or a source directory (with main.ax).
";

/// Parse `args`, dispatch the command, and return the process exit code. This is
/// the whole driver; `main` is a one-line shell over it.
pub fn run(args: &[String]) -> ExitCode {
    match parse_args(args) {
        Ok(Command::Check { path }) => run_check(&path),
        Ok(Command::Run { path }) => run_run(&path),
        Ok(Command::Build { .. }) => {
            unimplemented_command("build", "M5 (the Cranelift native backend)")
        }
        Ok(Command::Help) => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("axiom {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            eprint!("\n{HELP}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Read the file, check it, print the CST, HIR, and THIR to stdout and
/// diagnostics to stderr.
fn run_check(path: &Path) -> ExitCode {
    if path.is_dir() {
        return run_check_dir(path);
    }
    let source = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", path.display());
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let stdlib_exports = build_stdlib_exports();
    let report = compile_source(&source, stdlib_exports.as_ref()).report;
    print!(
        "{}\n{}\n{}",
        report.tree_dump, report.hir_dump, report.thir_dump
    );
    for diagnostic in &report.diagnostics {
        eprintln!("{diagnostic}");
    }
    if report.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_DIAGNOSTICS)
    }
}

/// Multi-file check: build the module graph, compile each module, and combine.
/// Per-module data collected during structural lowering.
type ModuleData = (
    String,
    Vec<axiom_hir::Item>,
    Vec<axiom_hir::Def>,
    Vec<axiom_hir::HirDiagnostic>,
);

fn run_check_dir(path: &Path) -> ExitCode {
    let src_dir = path.join("src");
    let search_dir = if src_dir.exists() { &src_dir } else { path };
    let mut graph = match axiom_modules::discover::discover(search_dir) {
        Ok(g) => g,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    // Merge stdlib modules into the graph.
    if let Some(stdlib) = stdlib_dir() {
        match axiom_modules::discover::discover_library(&stdlib) {
            Ok(stdlib_graph) => graph.merge(stdlib_graph),
            Err(err) => {
                eprintln!("warning: could not load stdlib: {err}");
            }
        }
    }

    // Phase 1: structural lowering for all modules (no name resolution yet).
    let (mut module_data, mut any_errors) = lower_all_modules(&graph);

    // Phase 2: build global export map from all modules' pub items.
    let export_input: Vec<(String, Vec<axiom_hir::Def>)> = module_data
        .iter()
        .map(|(name, _, defs, _)| (name.clone(), (*defs).clone()))
        .collect();
    let global_exports = axiom_hir::build_global_exports(&export_input);

    // Phase 3: resolve each module with cross-module context.
    let all_items = resolve_all_modules(&mut module_data, &global_exports, &mut any_errors);

    // Phase 4: type-check the combined HIR.
    any_errors |= typecheck_combined(all_items);

    if any_errors {
        ExitCode::from(EXIT_DIAGNOSTICS)
    } else {
        ExitCode::SUCCESS
    }
}

/// Phase 1: parse and structurally lower all modules.
fn lower_all_modules(graph: &axiom_modules::graph::ModuleGraph) -> (Vec<ModuleData>, bool) {
    let mut next_id = 0usize;
    let mut module_data = Vec::new();
    let mut any_errors = false;

    for module_id in graph.topo_order() {
        let module = graph.get(module_id);
        let parse_result = axiom_parser::parse(&module.source);
        let Some(root) = axiom_parser::ast::SourceFile::cast(parse_result.tree) else {
            eprintln!("error: failed to parse module `{}`", module.name);
            any_errors = true;
            continue;
        };
        let (items, defs, diags, nid) = axiom_hir::lower_structural(&root, &module.source, next_id);
        next_id = nid;
        for diag in &diags {
            eprintln!("{diag}");
            any_errors = true;
        }
        module_data.push((module.name.clone(), items, defs, diags));
    }

    (module_data, any_errors)
}

/// Phase 3: resolve names in all modules using cross-module exports.
fn resolve_all_modules(
    module_data: &mut [ModuleData],
    global_exports: &axiom_hir::GlobalExports,
    any_errors: &mut bool,
) -> Vec<axiom_hir::Item> {
    let mut all_items = Vec::new();

    for (module_name, items, defs, module_diags) in module_data {
        let mut items = std::mem::take(items);
        let mut diagnostics = std::mem::take(module_diags);
        axiom_hir::resolve_with_globals(
            &mut items,
            defs,
            &mut diagnostics,
            global_exports,
            module_name,
        );
        for diag in &diagnostics {
            eprintln!("{diag}");
            *any_errors = true;
        }
        all_items.extend(items);
    }

    all_items
}

/// Phase 4: type-check the combined HIR. Returns true if there were errors.
fn typecheck_combined(items: Vec<axiom_hir::Item>) -> bool {
    let combined_hir = axiom_hir::Hir {
        items,
        diagnostics: Vec::new(),
    };
    let thir = axiom_typeck::check(combined_hir);
    let mut any_errors = false;
    for diag in &thir.diagnostics {
        eprintln!("{diag}");
        any_errors = true;
    }
    any_errors
}

/// Read the file, compile through IR, and execute in the VM.
fn run_run(path: &Path) -> ExitCode {
    if path.is_dir() {
        return run_run_dir(path);
    }
    let source = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", path.display());
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let stdlib_exports = build_stdlib_exports();
    let compiled = compile_source(&source, stdlib_exports.as_ref());
    for diagnostic in &compiled.report.diagnostics {
        eprintln!("{diagnostic}");
    }
    if !compiled.report.is_clean() {
        return ExitCode::from(EXIT_DIAGNOSTICS);
    }
    let thir = match compiled.thir {
        Some(t) => t,
        None => return ExitCode::from(EXIT_DIAGNOSTICS),
    };
    let mono = axiom_typeck::monomorphize(&thir);
    let ir = axiom_ir::lower(&thir, &mono);
    let mut vm = axiom_vm::Vm::new(ir);
    match vm.run() {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(EXIT_DIAGNOSTICS)
        }
    }
}

/// Compile a multi-file project and run it in the VM.
fn run_run_dir(path: &Path) -> ExitCode {
    let src_dir = path.join("src");
    let search_dir = if src_dir.exists() { &src_dir } else { path };
    let mut graph = match axiom_modules::discover::discover(search_dir) {
        Ok(g) => g,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    // Merge stdlib modules into the graph.
    if let Some(stdlib) = stdlib_dir() {
        match axiom_modules::discover::discover_library(&stdlib) {
            Ok(stdlib_graph) => graph.merge(stdlib_graph),
            Err(err) => {
                eprintln!("warning: could not load stdlib: {err}");
            }
        }
    }

    // Phase 1: structural lowering for all modules.
    let (mut module_data, mut any_errors) = lower_all_modules(&graph);

    // Phase 2: build global export map.
    let export_input: Vec<(String, Vec<axiom_hir::Def>)> = module_data
        .iter()
        .map(|(name, _, defs, _)| (name.clone(), (*defs).clone()))
        .collect();
    let global_exports = axiom_hir::build_global_exports(&export_input);

    // Phase 3: resolve each module with cross-module context.
    let all_items = resolve_all_modules(&mut module_data, &global_exports, &mut any_errors);

    // Phase 4: type-check the combined HIR.
    let combined_hir = axiom_hir::Hir {
        items: all_items,
        diagnostics: Vec::new(),
    };
    let thir = axiom_typeck::check(combined_hir);
    for diag in &thir.diagnostics {
        eprintln!("{diag}");
        any_errors = true;
    }
    if any_errors {
        return ExitCode::from(EXIT_DIAGNOSTICS);
    }

    // Phase 5: monomorphize, lower to IR, and execute.
    let mono = axiom_typeck::monomorphize(&thir);
    let ir = axiom_ir::lower(&thir, &mono);
    let mut vm = axiom_vm::Vm::new(ir);
    match vm.run() {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(EXIT_DIAGNOSTICS)
        }
    }
}

/// Locate the stdlib directory. Looks relative to the workspace root
/// (parent of `crates/`), falling back to `CARGO_MANIFEST_DIR`/../stdlib.
fn stdlib_dir() -> Option<PathBuf> {
    // At compile time, CARGO_MANIFEST_DIR points to crates/axiom-cli/.
    // The workspace root is one level up.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest.parent()?; // crates/
    let workspace_root = workspace_root.parent()?; // workspace root
    let stdlib = workspace_root.join("stdlib");
    if stdlib.exists() {
        Some(stdlib)
    } else {
        None
    }
}

/// Build global exports from stdlib modules. Used by the single-file
/// compilation path so `println` (and other stdlib items) resolve without
/// an explicit `use` statement.
pub fn build_stdlib_exports() -> Option<axiom_hir::GlobalExports> {
    let stdlib = stdlib_dir()?;
    let graph = axiom_modules::discover::discover_library(&stdlib).ok()?;
    let mut module_data = Vec::new();
    for module_id in graph.topo_order() {
        let module = graph.get(module_id);
        if module.source.is_empty() {
            continue;
        }
        let parse_result = axiom_parser::parse(&module.source);
        let Some(root) = axiom_parser::ast::SourceFile::cast(parse_result.tree) else {
            continue;
        };
        let (items, defs, _diags, _nid) = axiom_hir::lower_structural(&root, &module.source, 0);
        let _ = items; // only defs needed for exports
        module_data.push((module.name.clone(), defs));
    }
    Some(axiom_hir::build_global_exports(&module_data))
}

/// Report a recognized-but-not-yet-built command and the milestone that lands it.
fn unimplemented_command(name: &str, milestone: &str) -> ExitCode {
    eprintln!("error: `axiom {name}` is not implemented yet — arrives in {milestone}.");
    ExitCode::from(EXIT_UNIMPLEMENTED)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args_run() {
        let args = vec!["run".to_string(), "test.ax".to_string()];
        let cmd = parse_args(&args).unwrap();
        assert!(matches!(cmd, Command::Run { .. }));
    }

    #[test]
    fn test_parse_args_build() {
        let args = vec!["build".to_string(), "test.ax".to_string()];
        let cmd = parse_args(&args).unwrap();
        assert!(matches!(cmd, Command::Build { .. }));
    }

    #[test]
    fn test_check_source_clean() {
        let report = check_source("fn main() { val x = 1 }");
        assert!(report.is_clean(), "diagnostics: {:?}", report.diagnostics);
    }

    #[test]
    fn test_check_source_type_error() {
        let report = check_source("fn main() { val x: Int = 3.14 }");
        assert!(!report.is_clean());
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.contains("type mismatch")));
    }
}
