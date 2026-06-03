//! Argument parsing: turn `&[String]` into a [`Command`]. Hand-rolled (the v0
//! surface is three subcommands plus help/version), so no CLI dependency. Pure
//! and total — every input maps to a `Command` or a [`CliError`], never a panic.

use std::path::PathBuf;

/// A fully-parsed invocation of the `axiom` driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `axiom check <file>` — lex + parse, then report diagnostics (M0).
    Check { path: PathBuf },
    /// `axiom run <file>` — execute via the IR interpreter (arrives in M4).
    Run { path: PathBuf },
    /// `axiom build <file>` — compile to a native executable (arrives in M5).
    Build { path: PathBuf },
    /// `axiom help` / `-h` / `--help`.
    Help,
    /// `axiom version` / `-V` / `--version`.
    Version,
}

/// Why an argument list could not be turned into a [`Command`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliError {
    #[error("no command given")]
    NoCommand,
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("`{0}` needs a file path, e.g. `axiom {0} hello.ax`")]
    MissingPath(&'static str),
}

/// Parse the driver's arguments (the program name already stripped).
pub fn parse_args(args: &[String]) -> Result<Command, CliError> {
    let mut rest = args.iter();
    let Some(command) = rest.next() else {
        return Err(CliError::NoCommand);
    };
    match command.as_str() {
        "check" => one_path(rest, "check").map(|path| Command::Check { path }),
        "run" => one_path(rest, "run").map(|path| Command::Run { path }),
        "build" => one_path(rest, "build").map(|path| Command::Build { path }),
        "help" | "-h" | "--help" => Ok(Command::Help),
        "version" | "-V" | "--version" => Ok(Command::Version),
        other => Err(CliError::UnknownCommand(other.to_string())),
    }
}

/// The next argument as a path, or a `MissingPath` error naming the subcommand.
fn one_path<'a>(
    mut rest: impl Iterator<Item = &'a String>,
    command: &'static str,
) -> Result<PathBuf, CliError> {
    match rest.next() {
        Some(arg) => Ok(PathBuf::from(arg)),
        None => Err(CliError::MissingPath(command)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_parse_args_check_with_path() {
        let cmd = parse_args(&args(&["check", "hello.ax"]));
        assert_eq!(
            cmd,
            Ok(Command::Check {
                path: PathBuf::from("hello.ax")
            })
        );
    }

    #[test]
    fn test_parse_args_run_and_build() {
        assert_eq!(
            parse_args(&args(&["run", "a.ax"])),
            Ok(Command::Run {
                path: PathBuf::from("a.ax")
            })
        );
        assert_eq!(
            parse_args(&args(&["build", "a.ax"])),
            Ok(Command::Build {
                path: PathBuf::from("a.ax")
            })
        );
    }

    #[test]
    fn test_parse_args_help_and_version_aliases() {
        for flag in ["help", "-h", "--help"] {
            assert_eq!(parse_args(&args(&[flag])), Ok(Command::Help));
        }
        for flag in ["version", "-V", "--version"] {
            assert_eq!(parse_args(&args(&[flag])), Ok(Command::Version));
        }
    }

    #[test]
    fn test_parse_args_no_command() {
        assert_eq!(parse_args(&args(&[])), Err(CliError::NoCommand));
    }

    #[test]
    fn test_parse_args_unknown_command() {
        assert_eq!(
            parse_args(&args(&["frobnicate", "x.ax"])),
            Err(CliError::UnknownCommand("frobnicate".to_string()))
        );
    }

    #[test]
    fn test_parse_args_missing_path() {
        assert_eq!(
            parse_args(&args(&["check"])),
            Err(CliError::MissingPath("check"))
        );
    }
}
