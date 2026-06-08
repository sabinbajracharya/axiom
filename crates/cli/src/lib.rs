//! The Axiom compiler driver (`axiom`). Owns the user-facing command surface
//! and the `.ax` feature-test harness.
//!
//! At **M2** the `check` command runs lex + parse + HIR lowering + name
//! resolution + type checking, printing CST, HIR, and THIR dumps. Type
//! errors appear as `TypeDiagnostic`s alongside HIR diagnostics.
//!
//! ```
//! use cli::check_source;
//! let report = check_source("fn main() { val x = 1 + 2 }");
//! assert!(report.is_clean());
//! ```

mod check;
pub mod cli;
pub mod harness;

pub use check::{check_source, compile_source, CheckReport, CompileResult};
pub use cli::{parse_args, CliError, Command};

use std::path::Path;
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
    let report = compile_source(&source, true).report;
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

/// Compile a project directory: discover the **user** module graph, then compile
/// it on top of the embedded stdlib through the one unified `check_modules`
/// pipeline. The stdlib is no longer disk-discovered or merged into the user
/// graph — it is embedded. See `docs/stdlib-loading-unification.md`.
fn compile_dir(search_dir: &Path) -> Result<typecheck::Thir, ExitCode> {
    let graph = match modules::discover::discover(search_dir) {
        Ok(g) => g,
        Err(err) => {
            eprintln!("error: {err}");
            return Err(ExitCode::from(EXIT_USAGE));
        }
    };
    // User modules are the source-bearing entries (skip synthetic dir modules).
    let user_modules: Vec<(String, String)> = graph
        .topo_order()
        .iter()
        .map(|id| graph.get(*id))
        .filter(|m| !m.source.is_empty())
        .map(|m| (m.name.clone(), m.source.clone()))
        .collect();
    let mut modules: Vec<(&str, &str)> = stdlib::modules().to_vec();
    for (name, source) in &user_modules {
        modules.push((name, source));
    }
    Ok(driver::check_modules(&modules))
}

/// Print all diagnostics from a compiled `Thir`. Returns true if any were emitted.
fn print_thir_diagnostics(thir: &typecheck::Thir) -> bool {
    let mut any = false;
    for diag in &thir.diagnostics {
        eprintln!("{diag}");
        any = true;
    }
    any
}

/// Multi-file check: discover the user graph, compile on the embedded stdlib,
/// and report diagnostics.
fn run_check_dir(path: &Path) -> ExitCode {
    let src_dir = path.join("src");
    let search_dir = if src_dir.exists() { &src_dir } else { path };
    let thir = match compile_dir(search_dir) {
        Ok(t) => t,
        Err(code) => return code,
    };
    if print_thir_diagnostics(&thir) {
        ExitCode::from(EXIT_DIAGNOSTICS)
    } else {
        ExitCode::SUCCESS
    }
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
    let compiled = compile_source(&source, true);
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
    let mono = specialize::monomorphize(&thir);
    let ir = ir::lower(&thir, &mono);
    let mut vm = vm::Vm::new(ir);
    match vm.run() {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(EXIT_DIAGNOSTICS)
        }
    }
}

/// Compile a multi-file project on the embedded stdlib and run it in the VM.
fn run_run_dir(path: &Path) -> ExitCode {
    let src_dir = path.join("src");
    let search_dir = if src_dir.exists() { &src_dir } else { path };
    let thir = match compile_dir(search_dir) {
        Ok(t) => t,
        Err(code) => return code,
    };
    if print_thir_diagnostics(&thir) {
        return ExitCode::from(EXIT_DIAGNOSTICS);
    }

    // Monomorphize, lower to IR, and execute.
    let mono = specialize::monomorphize(&thir);
    let ir = ir::lower(&thir, &mono);
    let mut vm = vm::Vm::new(ir);
    match vm.run() {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(EXIT_DIAGNOSTICS)
        }
    }
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
