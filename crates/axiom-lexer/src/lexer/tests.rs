// Tests legitimately panic on failure (assert/expect). RUST_CONVENTIONS §3.4.
#![allow(clippy::expect_used)]

use super::*;
use crate::invariants::check_all;
use crate::token::{Keyword, Punct};

/// Lex and assert the coverage invariants hold (every test exercises §4).
fn kinds(src: &str) -> Vec<TokenKind> {
    let result = lex(src);
    check_all(&result.tokens, src).expect("invariants must hold");
    result.tokens.into_iter().map(|t| t.kind).collect()
}

#[test]
fn test_let_binding() {
    assert_eq!(
        kinds("let x = 5"),
        vec![
            TokenKind::Keyword(Keyword::Let),
            TokenKind::Whitespace,
            TokenKind::Ident,
            TokenKind::Whitespace,
            TokenKind::Punct(Punct::Eq),
            TokenKind::Whitespace,
            TokenKind::IntLit(5),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_radix_integers() {
    assert_eq!(kinds("0xFF")[0], TokenKind::IntLit(255));
    assert_eq!(kinds("0o17")[0], TokenKind::IntLit(15));
    assert_eq!(kinds("0b1010")[0], TokenKind::IntLit(10));
    assert_eq!(kinds("1_000_000")[0], TokenKind::IntLit(1_000_000));
}

#[test]
fn test_floats() {
    assert_eq!(kinds("2.5")[0], TokenKind::FloatLit(2.5));
    assert_eq!(kinds("1e-9")[0], TokenKind::FloatLit(1e-9));
    assert_eq!(kinds("6.022e23")[0], TokenKind::FloatLit(6.022e23));
}

#[test]
fn test_range_is_not_a_float() {
    // `1..5` must be Int, DotDot, Int — not `1.` `.5`.
    assert_eq!(
        kinds("1..5"),
        vec![
            TokenKind::IntLit(1),
            TokenKind::Punct(Punct::DotDot),
            TokenKind::IntLit(5),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_maximal_munch_operators() {
    assert_eq!(kinds(">>")[0], TokenKind::Punct(Punct::Shr));
    assert_eq!(kinds("..=")[0], TokenKind::Punct(Punct::DotDotEq));
    assert_eq!(kinds("=>")[0], TokenKind::Punct(Punct::FatArrow));
    assert_eq!(kinds("->")[0], TokenKind::Punct(Punct::Arrow));
    assert_eq!(kinds("::")[0], TokenKind::Punct(Punct::ColonColon));
}

#[test]
fn test_division_operators() {
    // Regression: `/` must lex as the division operator, not Unknown.
    let result = lex("a / b");
    assert!(
        result.errors.is_empty(),
        "division must lex cleanly: {:?}",
        result.errors
    );
    assert_eq!(kinds("/")[0], TokenKind::Punct(Punct::Slash));
    assert_eq!(kinds("/=")[0], TokenKind::Punct(Punct::SlashEq));
    // And `/` next to comments stays a comment.
    assert_eq!(kinds("//x")[0], TokenKind::LineComment);
    assert_eq!(kinds("/* x */")[0], TokenKind::BlockComment);
}

#[test]
fn test_compound_assignment_operators() {
    // Compound assignment must lex as single tokens, not two adjacent tokens.
    assert_eq!(kinds("+=")[0], TokenKind::Punct(Punct::PlusEq));
    assert_eq!(kinds("-=")[0], TokenKind::Punct(Punct::MinusEq));
    assert_eq!(kinds("*=")[0], TokenKind::Punct(Punct::StarEq));
    assert_eq!(kinds("%=")[0], TokenKind::Punct(Punct::PercentEq));
    assert_eq!(kinds("/=")[0], TokenKind::Punct(Punct::SlashEq));
}

#[test]
fn test_empty_byte_literal_does_not_eat_quote() {
    let result = lex("b''");
    // The closing quote must not be consumed as the value.
    assert_eq!(result.tokens[0].kind, TokenKind::ByteLit(0));
    assert_eq!(result.errors.len(), 1);
}

#[test]
fn test_byte_literal_rejects_raw_newline() {
    let result = lex("b'\n'");
    assert!(!result.errors.is_empty());
}

#[test]
fn test_string_escapes_decoded() {
    assert_eq!(kinds("\"a\\nb\"")[0], TokenKind::StrLit("a\nb".to_string()));
    assert_eq!(kinds("\"\\u{41}\"")[0], TokenKind::StrLit("A".to_string()));
}

#[test]
fn test_raw_string_no_escapes() {
    assert_eq!(
        kinds("r\"a\\nb\"")[0],
        TokenKind::StrLit("a\\nb".to_string())
    );
}

#[test]
fn test_invalid_escape_preserves_source_text() {
    // The decoded value keeps the raw "\q" rather than silently dropping it.
    let result = lex("\"a\\qb\"");
    assert_eq!(
        result.tokens[0].kind,
        TokenKind::StrLit("a\\qb".to_string())
    );
    assert_eq!(result.errors.len(), 1);
}

#[test]
fn test_leading_bom_is_whitespace_not_error() {
    let result = lex("\u{feff}val");
    assert!(
        result.errors.is_empty(),
        "BOM must not error: {:?}",
        result.errors
    );
    assert_eq!(result.tokens[0].kind, TokenKind::Whitespace);
    assert_eq!(result.tokens[1].kind, TokenKind::Keyword(Keyword::Val));
}

#[test]
fn test_byte_literal() {
    assert_eq!(kinds("b'A'")[0], TokenKind::ByteLit(65));
    assert_eq!(kinds("b'\\n'")[0], TokenKind::ByteLit(10));
}

#[test]
fn test_comments() {
    assert_eq!(kinds("// hi")[0], TokenKind::LineComment);
    assert_eq!(kinds("/// doc")[0], TokenKind::DocComment);
    assert_eq!(kinds("/* a /* nested */ b */")[0], TokenKind::BlockComment);
}

#[test]
fn test_unterminated_string_reports_error_but_tiles() {
    let result = lex("\"oops");
    check_all(&result.tokens, "\"oops").expect("must still tile");
    assert_eq!(result.errors.len(), 1);
}

#[test]
fn test_unknown_char_becomes_unknown_token() {
    let result = lex("@");
    assert_eq!(result.tokens[0].kind, TokenKind::Unknown);
    assert_eq!(result.errors.len(), 1);
}

#[test]
fn test_self_and_self_type_are_distinct() {
    assert_eq!(kinds("self")[0], TokenKind::Keyword(Keyword::SelfValue));
    assert_eq!(kinds("Self")[0], TokenKind::Keyword(Keyword::SelfType));
}

#[test]
fn test_empty_source_is_just_eof() {
    assert_eq!(kinds(""), vec![TokenKind::Eof]);
}

#[test]
fn test_question_mark_token() {
    assert_eq!(kinds("?")[0], TokenKind::Punct(Punct::Question));
    // Postfix `?` next to an identifier.
    assert_eq!(
        kinds("x?"),
        vec![
            TokenKind::Ident,
            TokenKind::Punct(Punct::Question),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_loop_label_token() {
    assert_eq!(kinds("'outer"), vec![TokenKind::Label, TokenKind::Eof]);
    // A label followed by a colon (labeled loop) and inside `break`.
    assert_eq!(
        kinds("'a:"),
        vec![
            TokenKind::Label,
            TokenKind::Punct(Punct::Colon),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_lone_quote_is_unknown_but_tiles() {
    let result = lex("'");
    assert_eq!(result.tokens[0].kind, TokenKind::Unknown);
    assert_eq!(result.errors.len(), 1);
}

#[test]
fn test_byte_literal_still_lexes_after_label_support() {
    // Regression: `b'A'` must remain a byte literal, not a label.
    assert_eq!(kinds("b'A'")[0], TokenKind::ByteLit(65));
}
