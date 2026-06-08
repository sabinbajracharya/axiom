//! Fuzz / no-panic / no-hang tests (`docs/parser-testing.md` §5, Layer 5). A
//! std-only, fixed-seed PRNG generates thousands of inputs — both raw character
//! soup and random sequences of real Axiom token fragments. For each: parsing
//! must not panic, the tree must satisfy the coverage invariants (round-trip +
//! tiling + token coverage), and the run must terminate (a hang would fail the
//! test by timing out). No external dependency.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use parser::{check_all, parse};

/// Deterministic xorshift64 PRNG (fixed seed → reproducible failures).
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[(self.next_u64() as usize) % items.len()]
    }
}

/// Characters biased toward parser edge cases.
const POOL: &[char] = &[
    'a', 'Z', '_', '0', '9', ' ', '\n', '"', '\\', '/', '*', '.', '=', '<', '>', '&', '|', '^',
    '!', '+', '-', '%', ':', ';', '(', ')', '{', '}', '[', ']', ',', 'é', '😀',
];

/// Real token fragments, so the grammar's productions actually fire.
const FRAGMENTS: &[&str] = &[
    "fn", "struct", "enum", "trait", "impl", "val", "var", "let", "inout", "sink", "match", "if",
    "else", "loop", "return", "try", "catch", "mod", "use", "pub", "scope", "const", "for", "in",
    "as", "->", "=>", "::", "..", "..=", "||", "&&", "==", "(", ")", "{", "}", "[", "]", "<", ">",
    "x", "Foo", "42", "1.5", "\"s\"", ".", ",", ":", ";", "=", "+", "self", "Self",
];

fn random_chars(rng: &mut Rng, max_len: usize) -> String {
    let len = (rng.next_u64() as usize) % max_len;
    (0..len).map(|_| rng.pick(POOL)).collect()
}

fn random_program(rng: &mut Rng, max_frags: usize) -> String {
    let n = (rng.next_u64() as usize) % max_frags;
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(rng.pick(FRAGMENTS));
        s.push(' ');
    }
    s
}

fn assert_total(source: &str) {
    let result = parse(source);
    let tokens = lexer::lex(source).tokens;
    if let Err(reason) = check_all(&result.tree, source, &tokens) {
        panic!("invariant failed on input {source:?}: {reason}");
    }
}

#[test]
fn fuzz_char_soup_is_total() {
    let mut rng = Rng(0x5EED_0000_0000_0001);
    for _ in 0..20_000 {
        assert_total(&random_chars(&mut rng, 80));
    }
}

#[test]
fn fuzz_token_fragments_is_total() {
    let mut rng = Rng(0xA11CE_u64.wrapping_mul(0x9E37_79B9));
    for _ in 0..20_000 {
        assert_total(&random_program(&mut rng, 40));
    }
}

#[test]
fn adversarial_inputs_are_total() {
    let cases = [
        "",
        "{",
        "}",
        "((((((((((",
        "fn fn fn fn",
        "struct S {",
        "match x {",
        "fn f() { val x = ",
        "impl T for ",
        "use a::b::{c, d::{e",
        "1 + + + 2",
        "|||||",
        "..=..=..=",
        "fn f<T: A + B + >() {}",
    ];
    for case in cases {
        assert_total(case);
    }
}

#[test]
fn deeply_nested_input_terminates_and_round_trips() {
    // Round-trip on moderately deep input. The recursion guard keeps genuine
    // nesting shallow (it recovers past the limit), and the wide chain stays
    // within the recursive consumers' safe depth (see the guard test below for
    // the pathological cases). A hang would fail this by never returning.
    let deep = "fn f() ".to_string() + &"{ if x ".repeat(300) + &"{}".repeat(300);
    assert_total(&deep);

    // Wide left-associative chains build a left-leaning tree (depth = operator
    // count) via the iterative Pratt loop. `check_all` walks it with an explicit
    // work-stack (iterative), and `Rc` drop is iterative (both green and red
    // trees), so neither serializing/checking nor dropping it ever overflows
    // regardless of depth.
    let wide = "fn f() { ".to_string() + &"a + ".repeat(1_000) + "b }";
    assert_total(&wide);
}

#[test]
fn pathological_nesting_recovers_via_depth_guard() {
    // The recursion-depth guard must cover EVERY recursive grammar path —
    // expressions, blocks, types, error-union types, patterns/calls, and
    // use-trees — or deeply nested input overflows the parser's stack (a
    // totality violation). Each input nests far past `MAX_DEPTH`; we assert
    // parsing RETURNS and emitted a "too deep" diagnostic (the guard fired).
    //
    // The red-tree consumers (`check_all`, serializer) are iterative, so we also
    // run the full invariant check on the deep recovery tree. Parsing itself is
    // total — the grammar recursion is guarded and `build_tree` plus both green-
    // and red-tree `Drop` are iterative — so neither parsing nor checking crashes.
    let cases = [
        format!("fn f(x: {}T{}) {{}}", "A<".repeat(5_000), ">".repeat(5_000)), // types
        format!("fn f(x: {}A) {{}}", "A!".repeat(5_000)),                      // error-union types
        format!(
            "fn f() {{ match x {{ {}y{} => 1 }} }}",
            "S(".repeat(5_000),
            ")".repeat(5_000)
        ), // patterns + nested calls
        format!("use {}x{};", "a::{".repeat(5_000), "}".repeat(5_000)),        // use-trees
        format!("fn f() {}{}", "{ if x ".repeat(5_000), "{}".repeat(5_000)),   // blocks + expr
    ];
    for src in cases {
        let result = parse(&src);
        assert!(
            result.errors.iter().any(|e| e.message.contains("too deep")),
            "deeply nested input should trip the recursion guard, got {} errors",
            result.errors.len()
        );
        let tokens = lexer::lex(&src).tokens;
        if let Err(reason) = check_all(&result.tree, &src, &tokens) {
            panic!("invariant failed on deeply nested input: {reason}");
        }
    }
}
