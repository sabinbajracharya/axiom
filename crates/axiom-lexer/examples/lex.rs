//! The interactive debug dump: `cargo run -p axiom-lexer --example lex -- file.ax`.
//! Prints the canonical token snapshot (the same format the golden tests pin),
//! then any lexer diagnostics. This is the lexer's debugger until the real
//! `axiom` CLI exists (then it becomes `axiom debug tokens`).

use std::process::ExitCode;

use axiom_lexer::{lex, serialize};

fn main() -> ExitCode {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: lex <file.ax>");
            return ExitCode::FAILURE;
        }
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = lex(&source);
    print!("{}", serialize(&result.tokens, &source));

    if result.errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!("\n{} diagnostic(s):", result.errors.len());
        for err in &result.errors {
            let span = err.span();
            eprintln!("  bytes {}..{}: {err}", span.lo, span.hi);
        }
        ExitCode::FAILURE
    }
}
