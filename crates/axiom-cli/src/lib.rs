//! The Axiom compiler driver (`axiom`). Owns the user-facing command surface
//! and the `.ax` feature-test harness.
//!
//! At **M2** the `check` command runs lex + parse + HIR lowering + name
//! resolution + type checking, printing CST, HIR, and THIR dumps. Type
//! errors appear as `TypeDiagnostic`s alongside HIR diagnostics.
//!
//! ```
//! use axiom_cli::check_source;
//! let report = check_source("fn main() { print(\"hi\") }");
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
    axiom <command> [file.ax]

COMMANDS:
    check <file>    Lex, parse, and type-check; report diagnostics
    run <file>      Execute a program via the register-IR interpreter
    build <file>    Build a native executable (not yet implemented)
    help            Show this help
    version         Show the version

The package manager/build tool `forge` is a separate v2 concern.
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
    let source = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", path.display());
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let report = check_source(&source);
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

/// Read the file, compile through IR, and execute in the VM.
fn run_run(path: &Path) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", path.display());
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let compiled = compile_source(&source);
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
