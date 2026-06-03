//! The Axiom compiler driver (`axiom`). The first crate downstream of the
//! parser — it owns the user-facing command surface and the `.ax` feature-test
//! harness everything else plugs into.
//!
//! At **M0** the only working command is `axiom check <file>` (lex + parse, then
//! render diagnostics). `run` (M4, the IR interpreter) and `build` (M5, the
//! Cranelift native backend) are recognized but stubbed, so the command surface
//! is stable before the pipeline stages behind it exist.
//!
//! ```
//! use axiom_cli::check_source;
//! let report = check_source("fn main() { print(\"hi\") }");
//! assert!(report.is_clean());
//! ```
//!
//! The naming: `axiom` is the compiler driver. `forge` (the package manager /
//! build tool) is a separate v2 concern — deliberately not built here.

mod check;
pub mod cli;
pub mod harness;

pub use check::{check_source, CheckReport};
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
    check <file>    Lex and parse a source file; report diagnostics
    run <file>      Run a program (arrives in M4 — the IR interpreter)
    build <file>    Build a native executable (arrives in M5 — Cranelift)
    help            Show this help
    version         Show the version

The package manager/build tool `forge` is a separate v2 concern.
";

/// Parse `args`, dispatch the command, and return the process exit code. This is
/// the whole driver; `main` is a one-line shell over it.
pub fn run(args: &[String]) -> ExitCode {
    match parse_args(args) {
        Ok(Command::Check { path }) => run_check(&path),
        Ok(Command::Run { .. }) => unimplemented_command("run", "M4 (the IR interpreter)"),
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

/// Read the file, check it, print the CST to stdout and diagnostics to stderr.
fn run_check(path: &Path) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", path.display());
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let report = check_source(&source);
    print!("{}", report.tree_dump);
    for diagnostic in &report.diagnostics {
        eprintln!("{diagnostic}");
    }
    if report.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_DIAGNOSTICS)
    }
}

/// Report a recognized-but-not-yet-built command and the milestone that lands it.
fn unimplemented_command(name: &str, milestone: &str) -> ExitCode {
    eprintln!("error: `axiom {name}` is not implemented yet — arrives in {milestone}.");
    ExitCode::from(EXIT_UNIMPLEMENTED)
}
