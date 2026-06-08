//! Debug CLI for the type checker: reads a `.ax` file, runs the full
//! parse → lower → type-check pipeline, and prints the canonical THIR dump.
//!
//! Usage: `cargo run -p axiom-typeck --example typeck -- file.ax`

use parser::ast::AstNode;
use resolver::lower;
use typecheck::{check, serialize};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: typeck <file.ax>");
        std::process::exit(1);
    }
    let path = &args[0];
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", path);
            std::process::exit(1);
        }
    };

    let result = parser::parse(&source);
    for err in &result.errors {
        eprintln!("{}", err.render(&source));
    }

    let root = match parser::ast::SourceFile::cast(result.tree) {
        Some(r) => r,
        None => {
            eprintln!("error: parse result is not a SourceFile root");
            std::process::exit(1);
        }
    };

    let hir = lower(&root, &source, None);
    let thir = check(hir);

    let dump = serialize(&thir, None);
    print!("{dump}");

    for diag in &thir.diagnostics {
        eprintln!("{}", diag.render(&source));
    }

    if !thir.diagnostics.is_empty() {
        std::process::exit(1);
    }
}
