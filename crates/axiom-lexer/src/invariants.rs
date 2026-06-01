//! The coverage invariants (`docs/lexer-testing.md` §4) — defined once here and
//! reused by golden tests, unit tests, and the fuzzer. These are what make
//! "nothing is missed" a mechanical property rather than a promise.
//!
//! All three are pure functions over the token stream. They return
//! `Result<(), String>` so callers (and fuzz failures) get a precise reason.

use crate::token::Token;

/// `tiles`: the tokens partition the source with no gaps and no overlaps —
/// `tokens[0]` starts at 0, each token begins where the previous ended, spans
/// are well-formed, and the final position equals `source_len`.
pub fn tiles(tokens: &[Token], source_len: usize) -> Result<(), String> {
    let mut expected = 0usize;
    for (i, token) in tokens.iter().enumerate() {
        if token.span.hi < token.span.lo {
            return Err(format!(
                "token {i}: inverted span ({}..{})",
                token.span.lo, token.span.hi
            ));
        }
        if token.span.lo != expected {
            return Err(format!(
                "token {i}: gap/overlap — expected lo {expected}, got {}",
                token.span.lo
            ));
        }
        expected = token.span.hi;
    }
    if expected != source_len {
        return Err(format!(
            "stream ends at {expected}, source is {source_len} bytes"
        ));
    }
    Ok(())
}

/// `reconstruct`: concatenating every token's text reproduces the source. Works
/// on the token stream alone — the strongest lossless check.
pub fn reconstruct(tokens: &[Token]) -> String {
    let mut out = String::new();
    for token in tokens {
        out.push_str(&token.text);
    }
    out
}

/// Cross-check that each token's stored text equals the source slice its span
/// points at — catches a span and its text drifting apart.
pub fn spans_match_text(tokens: &[Token], source: &str) -> Result<(), String> {
    for (i, token) in tokens.iter().enumerate() {
        match source.get(token.span.lo..token.span.hi) {
            Some(slice) if slice == token.text => {}
            Some(slice) => {
                return Err(format!(
                    "token {i}: text {:?} != source slice {:?}",
                    token.text, slice
                ));
            }
            None => {
                return Err(format!(
                    "token {i}: span ({}..{}) is out of bounds or not on a char boundary",
                    token.span.lo, token.span.hi
                ));
            }
        }
    }
    Ok(())
}

/// Run all three invariants. The one call every test layer uses.
pub fn check_all(tokens: &[Token], source: &str) -> Result<(), String> {
    tiles(tokens, source.len())?;
    spans_match_text(tokens, source)?;
    let rebuilt = reconstruct(tokens);
    if rebuilt != source {
        return Err(format!(
            "reconstruction mismatch: {} bytes rebuilt vs {} source bytes",
            rebuilt.len(),
            source.len()
        ));
    }
    Ok(())
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
    fn test_tiles_accepts_contiguous_stream() {
        let toks = vec![
            tok(TokenKind::Ident, 0, 2, "ab"),
            tok(TokenKind::Eof, 2, 2, ""),
        ];
        assert!(tiles(&toks, 2).is_ok());
    }

    #[test]
    fn test_tiles_rejects_gap() {
        let toks = vec![
            tok(TokenKind::Ident, 0, 1, "a"),
            tok(TokenKind::Ident, 2, 3, "b"), // gap at byte 1
        ];
        assert!(tiles(&toks, 3).is_err());
    }

    #[test]
    fn test_tiles_rejects_short_coverage() {
        let toks = vec![tok(TokenKind::Ident, 0, 1, "a")];
        assert!(tiles(&toks, 5).is_err());
    }

    #[test]
    fn test_reconstruct_concatenates_text() {
        let toks = vec![
            tok(TokenKind::Ident, 0, 1, "a"),
            tok(TokenKind::Whitespace, 1, 2, " "),
            tok(TokenKind::Ident, 2, 3, "b"),
        ];
        assert_eq!(reconstruct(&toks), "a b");
    }

    #[test]
    fn test_spans_match_text_detects_drift() {
        let src = "ab";
        let good = vec![tok(TokenKind::Ident, 0, 2, "ab")];
        assert!(spans_match_text(&good, src).is_ok());
        let drifted = vec![tok(TokenKind::Ident, 0, 2, "xy")];
        assert!(spans_match_text(&drifted, src).is_err());
    }
}
