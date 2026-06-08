//! Example: lower a simple program to IR and print it.

use parser::ast::AstNode;

fn main() {
    let source = r#"
fn add(let a: Int, let b: Int) -> Int {
    a + b
}

fn main() {
    val x = add(1, 2)
}
"#;
    let result = parser::parse(source);
    let Some(root) = parser::ast::SourceFile::cast(result.tree) else {
        eprintln!("Failed to parse source");
        return;
    };
    let hir = resolver::lower(&root, source, None);
    let thir = typecheck::check(hir);
    let mono = typecheck::monomorphize(&thir);
    let ir = ir::lower(&thir, &mono);
    println!("{}", ir::serialize(&ir));
}
