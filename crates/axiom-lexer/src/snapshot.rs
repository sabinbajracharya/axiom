//! Canonical token snapshot serializer (`docs/lexer-testing.md` §2). Pure
//! functions: `&[Token] + source → String`. This output is both the debug dump
//! (`examples/lex.rs`) and the golden-test oracle, so the format is a contract.
//!
//! Kind names come exclusively from `symbols::display_name` — never spelled here
//! (enforced by `test_no_hardcoded_kind_labels`). Only escape sequences and
//! format punctuation are literals.

use crate::symbols::display_name;
use crate::token::{LineMap, Token, TokenKind};

/// Serialize a token stream to the canonical, diff-stable snapshot format: one
/// token per line, no cross-row alignment.
pub fn serialize(tokens: &[Token], source: &str) -> String {
    let lines = LineMap::new(source);
    let mut out = String::new();
    for (idx, token) in tokens.iter().enumerate() {
        out.push_str(&serialize_token(idx, token, source, &lines));
        out.push('\n');
    }
    out
}

/// One line: `[idx] Kind @ l:c-l:c (lo..hi) repr`.
fn serialize_token(idx: usize, token: &Token, source: &str, lines: &LineMap) -> String {
    let (l1, c1) = lines.locate(source, token.span.lo);
    let (l2, c2) = lines.locate(source, token.span.hi);
    format!(
        "[{idx}] {kind} @ {l1}:{c1}-{l2}:{c2} ({lo}..{hi}) {repr}",
        kind = display_name(&token.kind),
        lo = token.span.lo,
        hi = token.span.hi,
        repr = repr(token),
    )
}

/// The token's quoted, escaped text, plus a decoded `value=` for literals whose
/// value can differ from the raw text.
fn repr(token: &Token) -> String {
    let text = quote(&token.text);
    match &token.kind {
        TokenKind::IntLit(n) => format!("{text} value={n}"),
        TokenKind::FloatLit(x) => format!("{text} value={}", fmt_float(*x)),
        TokenKind::ByteLit(b) => format!("{text} value={b}"),
        TokenKind::StrLit(s) => format!("{text} value={}", quote(s)),
        _ => text,
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", escape(s))
}

/// Escape a string so a token always occupies exactly one snapshot line.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Canonical float text: specials spelled out, integer-valued floats get `.0`.
fn fmt_float(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        };
    }
    if n == 0.0 && n.is_sign_negative() {
        return "-0.0".to_string();
    }
    let s = format!("{n}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::{Span, TokenKind};

    fn tok(kind: TokenKind, lo: usize, hi: usize, text: &str) -> Token {
        Token {
            kind,
            span: Span { lo, hi },
            text: text.to_string(),
        }
    }

    #[test]
    fn test_serialize_line_format() {
        let toks = vec![
            tok(TokenKind::Ident, 0, 1, "x"),
            tok(TokenKind::Eof, 1, 1, ""),
        ];
        let out = serialize(&toks, "x");
        assert_eq!(
            out,
            "[0] Ident @ 1:1-1:2 (0..1) \"x\"\n[1] Eof @ 1:2-1:2 (1..1) \"\"\n"
        );
    }

    #[test]
    fn test_repr_appends_int_value() {
        let t = tok(TokenKind::IntLit(16), 0, 4, "0x10");
        assert_eq!(repr(&t), "\"0x10\" value=16");
    }

    #[test]
    fn test_repr_decoded_string_value() {
        let t = tok(TokenKind::StrLit("a\nb".to_string()), 0, 6, "\"a\\nb\"");
        // raw text and decoded value both escaped, but they differ
        assert_eq!(repr(&t), "\"\\\"a\\\\nb\\\"\" value=\"a\\nb\"");
    }

    #[test]
    fn test_fmt_float_specials_and_integers() {
        assert_eq!(fmt_float(1.0), "1.0");
        assert_eq!(fmt_float(1.5), "1.5");
        assert_eq!(fmt_float(f64::INFINITY), "inf");
        assert_eq!(fmt_float(-0.0), "-0.0");
    }

    #[test]
    fn test_no_hardcoded_kind_labels() {
        // The serializer must not spell any token-kind label as a literal — they
        // must come from symbols. Scan this file's own source for quoted labels.
        let src = include_str!("snapshot.rs");
        let mut labels: Vec<&str> = vec![
            display_name(&TokenKind::Whitespace),
            display_name(&TokenKind::Newline),
            display_name(&TokenKind::LineComment),
            display_name(&TokenKind::DocComment),
            display_name(&TokenKind::BlockComment),
            display_name(&TokenKind::IntLit(0)),
            display_name(&TokenKind::FloatLit(0.0)),
            display_name(&TokenKind::ByteLit(0)),
            display_name(&TokenKind::StrLit(String::new())),
            display_name(&TokenKind::Ident),
            display_name(&TokenKind::Unknown),
            display_name(&TokenKind::Eof),
        ];
        for kw in crate::token::Keyword::ALL {
            labels.push(crate::symbols::keyword_label(*kw));
        }
        for p in crate::token::Punct::ALL {
            labels.push(crate::symbols::punct_label(*p));
        }
        for label in labels {
            let needle = format!("\"{label}\"");
            assert!(
                !src.contains(&needle),
                "snapshot.rs hardcodes kind label {needle}; use symbols::display_name"
            );
        }
    }
}
