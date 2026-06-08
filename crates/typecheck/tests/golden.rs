//! Golden snapshot tests: `.ax` fixtures → `.thir` goldens.
//! Run with `UPDATE_SNAPSHOTS=1` to regenerate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use hir::lower;
use parser::ast::AstNode;
use typecheck::{check, monomorphize, serialize};

/// Build global exports from stdlib so `print`/`println` resolve.
fn stdlib_exports() -> Option<hir::GlobalExports> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent()?.parent()?;
    let stdlib = workspace.join("stdlib");
    if !stdlib.exists() {
        return None;
    }
    let graph = modules::discover::discover_library(&stdlib).ok()?;
    let mut module_data = Vec::new();
    for module_id in graph.topo_order() {
        let module = graph.get(module_id);
        if module.source.is_empty() {
            continue;
        }
        let parse_result = parser::parse(&module.source);
        let Some(root) = parser::ast::SourceFile::cast(parse_result.tree) else {
            continue;
        };
        let (_items, defs, _diags, _nid) = hir::lower_structural(&root, &module.source, 0);
        module_data.push((module.name.clone(), defs));
    }
    Some(hir::build_global_exports(&module_data))
}

fn typeck_source(source: &str) -> String {
    let exports = stdlib_exports();
    let result = parser::parse(source);
    let root = parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = lower(&root, source, exports.as_ref());
    let thir = check(hir);
    serialize(&thir, None)
}

fn typeck_source_with_mono(source: &str) -> String {
    let exports = stdlib_exports();
    let result = parser::parse(source);
    let root = parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = lower(&root, source, exports.as_ref());
    let thir = check(hir);
    let mono = monomorphize(&thir);
    serialize(&thir, Some(&mono))
}

fn read_golden(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn write_golden(path: &str, content: &str) {
    std::fs::write(path, content).unwrap();
}

fn check_golden(name: &str, source: &str) {
    let actual = typeck_source(source);
    let golden_path = format!("tests/fixtures/{}.thir", name);

    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        write_golden(&golden_path, &actual);
    } else {
        let expected = read_golden(&golden_path)
            .unwrap_or_else(|| panic!("golden file missing: {golden_path}"));
        assert_eq!(actual, expected, "golden mismatch for {name}");
    }
}

#[test]
fn test_golden_hello() {
    check_golden("hello", "fn main() { print(\"Hello, Axiom!\") }");
}

#[test]
fn test_golden_arithmetic() {
    check_golden("arithmetic", "fn add(a: Int, b: Int) -> Int { a + b }");
}

#[test]
fn test_golden_simple_struct() {
    check_golden(
        "simple_struct",
        "struct Point { x: Float, y: Float }
fn main() { val p = Point { x: 1.0, y: 2.0 } }",
    );
}

#[test]
fn test_golden_simple_enum() {
    check_golden(
        "simple_enum",
        "enum Shape { Circle(Float), Rect(Float, Float), Empty }",
    );
}

#[test]
fn test_golden_type_mismatch() {
    check_golden("type_mismatch", "fn main() { val x: Int = 3.14 }");
}

#[test]
fn test_golden_control_flow() {
    check_golden(
        "control_flow",
        "fn main() {
    val x = 1
    if x == 1 {
        val y = 2
    }
    val z = if x == 0 { 10 } else { 20 }
    loop {
        val done = true
    }
}",
    );
}

#[test]
fn test_golden_structs() {
    check_golden(
        "structs",
        "struct Point { x: Float, y: Float }
fn main() {
    val p = Point { x: 1.0, y: 2.0 }
    val px = p.x
    val py = p.y
}",
    );
}

#[test]
fn test_golden_enums() {
    check_golden(
        "enums",
        "enum Shape { Circle(Float), Rect(Float, Float), Empty }
fn describe(s: Shape) -> Float {
    match s {
        Circle(r) => 3.14 * r * r
        Rect(w, h) => w * h
        Empty => 0.0
    }
}",
    );
}

#[test]
fn test_golden_match_patterns() {
    check_golden(
        "match_patterns",
        "enum Color { Red, Green, Blue }
fn paint(c: Color) -> Float {
    match c {
        Red => 1.0
        Green => 2.0
        Blue => 3.0
    }
}",
    );
}

#[test]
fn test_golden_functions() {
    check_golden(
        "functions",
        "fn add(a: Int, b: Int) -> Int { a + b }
fn greet(name: String) -> String { name }
fn main() {
    val x = add(1, 2)
    val y = add(x, 3)
}",
    );
}

#[test]
fn test_golden_assignments() {
    check_golden(
        "assignments",
        "fn main() {
    val x = 1
    var y = 2
    y = 3
    val z: Int = x + y
}",
    );
}

#[test]
fn test_golden_methods() {
    check_golden(
        "methods",
        "struct Point {
    x: Float,
    y: Float,
}
impl Point {
    fn dist(let self, other: Point) -> Float {
        self.x
    }
}
fn main() {
    val p = Point { x: 3.0, y: 4.0 }
    val q = Point { x: 1.0, y: 2.0 }
    val d = p.dist(q)
    print(string::format(\"{}\", d))
}",
    );
}

#[test]
fn test_golden_structs_enums_match() {
    check_golden(
        "structs_enums_match",
        "enum Shape {
    Circle(Float),
    Rect(Float, Float),
    Empty,
}

fn area(s: Shape) -> Float {
    match s {
        Circle(r) => 3.14 * r * r
        Rect(w, h) => w * h
        Empty => 0.0
    }
}

fn main() {
    val c = Circle(3.0)
    val a = area(c)
    print(string::format(\"{}\", a))
}",
    );
}

#[test]
fn test_golden_struct_field_access() {
    check_golden(
        "struct_field_access",
        "struct Point {
    x: Float,
    y: Float,
}

fn origin() -> Point {
    Point { x: 0.0, y: 0.0 }
}

fn translate(p: Point, dx: Float, dy: Float) {
    p.x = p.x + dx
    p.y = p.y + dy
}",
    );
}

#[test]
fn test_golden_bindings() {
    check_golden(
        "bindings",
        "fn main() {
    val a: Int = 1
    val b: Float = 2.0
    val c: Bool = true
    val d: String = \"hello\"
}",
    );
}

#[test]
fn test_golden_break_value() {
    check_golden(
        "break_value",
        "fn main() {
    val x = loop {
        break 42
    }
}",
    );
}

#[test]
fn test_golden_break_no_value() {
    check_golden(
        "break_no_value",
        "fn main() {
    loop {
        break
    }
}",
    );
}

#[test]
fn test_golden_continue() {
    check_golden(
        "continue",
        "fn main() {
    loop {
        continue
    }
}",
    );
}

#[test]
fn test_golden_generics() {
    let source = "fn id<T>(x: T) -> T { x }
fn main() {
    val a = id(42)
}";
    let actual = typeck_source_with_mono(source);
    let golden_path = "tests/fixtures/generics.thir";
    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        std::fs::write(golden_path, &actual).unwrap();
    } else {
        let expected = std::fs::read_to_string(golden_path)
            .unwrap_or_else(|_| panic!("golden file missing: {golden_path}"));
        assert_eq!(actual, expected, "golden mismatch for generics");
    }
}

#[test]
fn test_golden_traits() {
    let source = "struct Circle { r: Float }
trait Shape {
    fn area(let self) -> Float;
    fn name(let self) -> String {
        \"shape\"
    }
}
impl Shape for Circle {
    fn area(let self) -> Float {
        3.14159 * self.r * self.r
    }
}
fn main() {
    val c = Circle { r: 3.0 }
    val a = c.area()
    print(string::format(\"{}\", a))
}";
    let actual = typeck_source(source);
    let golden_path = "tests/fixtures/traits.thir";
    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        std::fs::write(golden_path, &actual).unwrap();
    } else {
        let expected = std::fs::read_to_string(golden_path)
            .unwrap_or_else(|_| panic!("golden file missing: {golden_path}"));
        assert_eq!(actual, expected, "golden mismatch for traits");
    }
}
