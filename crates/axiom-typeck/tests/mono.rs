//! Integration tests for monomorphization.

use axiom_hir::lower;
use axiom_parser::ast::AstNode;
use axiom_typeck::Ty;

#[allow(clippy::unwrap_used)]
fn check_source(source: &str) -> axiom_typeck::Thir {
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = lower(&root, source, None);
    axiom_typeck::check(hir)
}

fn mono(source: &str) -> axiom_typeck::MonoResult {
    let thir = check_source(source);
    assert!(
        thir.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        thir.diagnostics
    );
    axiom_typeck::monomorphize(&thir)
}

// ── Basic monomorphization ────────────────────────────────────────────────────

#[test]
fn test_mono_identity_int() {
    let result = mono(
        "\
fn id<T>(x: T) -> T { x }
fn main() -> Int { id(42) }",
    );
    assert_eq!(result.instances.len(), 1, "expected 1 instance");
    let inst = &result.instances[0];
    assert_eq!(inst.name, "id__Int");
    assert_eq!(inst.original_name, "id");
    assert_eq!(inst.type_args, vec![Ty::Int]);
    assert_eq!(inst.param_types, vec![Ty::Int]);
    assert_eq!(inst.return_type, Ty::Int);
}

#[test]
fn test_mono_identity_string() {
    let result = mono(
        "\
fn id<T>(x: T) -> T { x }
fn main() -> String { id(\"hello\") }",
    );
    assert_eq!(result.instances.len(), 1);
    let inst = &result.instances[0];
    assert_eq!(inst.name, "id__String");
    assert_eq!(inst.param_types, vec![Ty::String]);
    assert_eq!(inst.return_type, Ty::String);
}

// ── Deduplication ─────────────────────────────────────────────────────────────

#[test]
fn test_mono_dedup() {
    let result = mono(
        "\
fn id<T>(x: T) -> T { x }
fn main() -> Int {
    val a = id(1)
    val b = id(2)
    a
}",
    );
    assert_eq!(
        result.instances.len(),
        1,
        "expected dedup: only 1 id__Int instance, got {}",
        result.instances.len()
    );
}

#[test]
fn test_mono_identity_two_types() {
    let result = mono(
        "\
fn id<T>(x: T) -> T { x }
fn main() -> Int {
    val _a = id(42)
    id(99)
}",
    );
    // Both calls use Int, so still 1 instance.
    assert_eq!(result.instances.len(), 1);
    assert_eq!(result.instances[0].name, "id__Int");
}

// ── No generics → no instances ────────────────────────────────────────────────

#[test]
fn test_mono_no_generics() {
    let result = mono(
        "\
fn add(a: Int, b: Int) -> Int { a }
fn main() -> Int { add(1, 2) }",
    );
    assert!(
        result.instances.is_empty(),
        "non-generic program should produce 0 instances"
    );
}

// ── Mangled name format ───────────────────────────────────────────────────────

#[test]
fn test_mono_mangled_name() {
    let result = mono(
        "\
fn wrap<T>(x: T) -> T { x }
fn main() -> Bool { wrap(true) }",
    );
    assert_eq!(result.instances.len(), 1);
    assert_eq!(result.instances[0].name, "wrap__Bool");
}

// ── Substituted signature ─────────────────────────────────────────────────────

#[test]
fn test_mono_substituted_signature() {
    let result = mono(
        "\
fn id<T>(x: T) -> T { x }
fn main() -> Float { id(3.14) }",
    );
    assert_eq!(result.instances.len(), 1);
    let inst = &result.instances[0];
    assert_eq!(inst.name, "id__Float");
    assert_eq!(inst.param_types, vec![Ty::Float]);
    assert_eq!(inst.return_type, Ty::Float);
    assert_eq!(inst.type_args, vec![Ty::Float]);
}

// ── Two type parameters ───────────────────────────────────────────────────────

#[test]
fn test_mono_two_type_params() {
    let result = mono(
        "\
fn pair<A, B>(a: A, b: B) -> A { a }
fn main() -> Int { pair(1, true) }",
    );
    assert_eq!(result.instances.len(), 1);
    let inst = &result.instances[0];
    assert_eq!(inst.name, "pair__Int_Bool");
    assert_eq!(inst.param_types, vec![Ty::Int, Ty::Bool]);
    assert_eq!(inst.return_type, Ty::Int);
    assert_eq!(inst.type_args, vec![Ty::Int, Ty::Bool]);
}

// ── Trait bound type ──────────────────────────────────────────────────────────

#[test]
fn test_mono_trait_bound_type() {
    let result = mono(
        "\
trait Ord { fn cmp(self) -> Int }
struct Foo { }
impl Ord for Foo { fn cmp(self) -> Int { 0 } }
fn take_ord<T: Ord>(x: T) -> Int { 0 }
fn main() -> Int { take_ord(Foo {}) }",
    );
    assert_eq!(result.instances.len(), 1);
    let inst = &result.instances[0];
    assert_eq!(inst.name, "take_ord__Foo");
    assert_eq!(inst.param_types.len(), 1);
    assert!(
        matches!(&inst.param_types[0], Ty::Struct(s) if s.name == "Foo"),
        "expected Struct(Foo), got {:?}",
        inst.param_types[0]
    );
    assert_eq!(inst.return_type, Ty::Int);
}

// ── Nested generic calls ──────────────────────────────────────────────────────

#[test]
fn test_mono_nested_generic_call() {
    let result = mono(
        "\
fn id<T>(x: T) -> T { x }
fn wrap<T>(x: T) -> T { id(x) }
fn main() -> Int { wrap(42) }",
    );
    // wrap<Int> discovers id<T> call in its body → id<Int> also created.
    let names: Vec<&str> = result.instances.iter().map(|i| i.name.as_str()).collect();
    assert!(
        names.contains(&"wrap__Int"),
        "expected wrap__Int, got {:?}",
        names
    );
    assert!(
        names.contains(&"id__Int"),
        "expected id__Int, got {:?}",
        names
    );
    assert_eq!(result.instances.len(), 2);
}

// ── Builtin types ─────────────────────────────────────────────────────────────

#[test]
fn test_mono_builtin_bool() {
    let result = mono(
        "\
fn id<T>(x: T) -> T { x }
fn main() -> Bool { id(true) }",
    );
    assert_eq!(result.instances.len(), 1);
    assert_eq!(result.instances[0].name, "id__Bool");
    assert_eq!(result.instances[0].param_types, vec![Ty::Bool]);
    assert_eq!(result.instances[0].return_type, Ty::Bool);
}

#[test]
fn test_mono_builtin_float() {
    let result = mono(
        "\
fn id<T>(x: T) -> T { x }
fn main() -> Float { id(1.5) }",
    );
    assert_eq!(result.instances.len(), 1);
    assert_eq!(result.instances[0].name, "id__Float");
    assert_eq!(result.instances[0].return_type, Ty::Float);
}
