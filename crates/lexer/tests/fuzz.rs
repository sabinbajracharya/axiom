//! Fuzz / no-panic tests (`docs/lexer-testing.md` §5, §8). A std-only,
//! fixed-seed PRNG generates thousands of inputs; for each, lexing must not
//! panic and the resulting stream must satisfy the coverage invariants. No
//! external dependency — consistent with the hand-rolled / minimal-deps stance.

// Integration tests legitimately panic on failure. RUST_CONVENTIONS §3.4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lexer::{check_all, lex};

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
        let i = (self.next_u64() as usize) % items.len();
        items[i]
    }
}

/// A pool biased toward characters that exercise lexer edge cases.
const POOL: &[char] = &[
    'a', 'Z', '_', '0', '9', ' ', '\t', '\n', '\r', '"', '\\', '\'', '/', '*', '.', '=', '<', '>',
    '&', '|', '^', '!', '+', '-', '%', ':', ';', '(', ')', '{', '}', '[', ']', 'r', 'b', 'x', 'o',
    'e', '{', '}', '@', '#', '$', '~', 'é', 'λ', '世', '😀',
];

fn random_string(rng: &mut Rng, max_len: usize) -> String {
    let len = (rng.next_u64() as usize) % max_len;
    let mut s = String::new();
    for _ in 0..len {
        s.push(rng.pick(POOL));
    }
    s
}

#[test]
fn fuzz_never_panics_and_always_tiles() {
    let mut rng = Rng(0x5EED_1234_ABCD_0001);
    for _ in 0..20_000 {
        let input = random_string(&mut rng, 64);
        let result = lex(&input);
        if let Err(reason) = check_all(&result.tokens, &input) {
            panic!("invariant failed on fuzz input {input:?}: {reason}");
        }
    }
}

#[test]
fn adversarial_inputs_tile() {
    let cases = [
        "",
        "\"",
        "\"\\",
        "\"\\u{",
        "\"\\u{zzzz}\"",
        "r\"",
        "b'",
        "b'\\",
        "/*",
        "/* /* /*",
        "*/",
        "0x",
        "0b",
        "1.",
        "1..",
        "1..=",
        "..=",
        "99999999999999999999999999",
        "\u{1F600}\u{1F600}\u{1F600}",
        "//no newline at eof",
        "///",
        "////",
    ];
    for case in cases {
        let result = lex(case);
        if let Err(reason) = check_all(&result.tokens, case) {
            panic!("invariant failed on adversarial input {case:?}: {reason}");
        }
    }
}

#[test]
fn huge_input_is_handled() {
    let big = "let x = 1\n".repeat(50_000);
    let result = lex(&big);
    assert!(check_all(&result.tokens, &big).is_ok());
}
