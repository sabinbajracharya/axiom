//! Example: lower a simple program to IR and print it.

use axiom_parser::ast::AstNode;

fn main() {
    let source = r#"
fn add(let a: Int, let b: Int) -> Int {
    a + b
}

fn main() {
    val x = add(1, 2)
}
"#;
    let result = axiom_parser::parse(source);
    let Some(root) = axiom_parser::ast::SourceFile::cast(result.tree) else {
        eprintln!("Failed to parse source");
        return;
    };
    let hir = axiom_hir::lower(&root, source);
    let thir = axiom_typeck::check(hir);
    let ir = axiom_ir::lower(&thir);
    println!("{}", axiom_ir::serialize(&ir));
}
