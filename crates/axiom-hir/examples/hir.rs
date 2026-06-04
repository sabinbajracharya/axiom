//! Debug HIR dump: `cargo run -p axiom-hir --example hir -- file.ax`.
//! Prints the canonical HIR snapshot to stdout and any diagnostics to stderr.

use axiom_parser::ast::AstNode;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: hir <file.ax>");
        return ExitCode::FAILURE;
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = axiom_parser::parse(&source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree);
    let Some(root) = root else {
        eprintln!("parse produced no SourceFile root");
        return ExitCode::FAILURE;
    };
    let hir = axiom_hir::lower(&root, &source);
    print!("{}", axiom_hir::serialize(&hir));
    for diag in &hir.diagnostics {
        eprintln!("{}", diag.render(&source));
    }
    if result.errors.is_empty() && hir.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
