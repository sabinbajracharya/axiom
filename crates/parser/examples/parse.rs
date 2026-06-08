//! Debug tree dump: `cargo run -p axiom-parser --example parse -- file.ax`.
//! Prints the canonical CST snapshot to stdout and any diagnostics to stderr.
//! This is the CLI face of the serializer until the real `axiom` CLI exists.

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: parse <file.ax>");
        return ExitCode::FAILURE;
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = parser::parse(&source);
    print!("{}", parser::serialize(&result.tree));
    for err in &result.errors {
        eprintln!("{}", err.render(&source));
    }
    ExitCode::SUCCESS
}
