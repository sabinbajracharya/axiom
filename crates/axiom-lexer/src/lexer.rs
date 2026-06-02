//! The scanner — the one deliberately stateful core (`docs/lexer-testing.md`
//! §5.1). A byte cursor walks the source; short single-purpose methods each scan
//! one token and return its `TokenKind`. Local mutation stays inside this struct.
//!
//! Lexing is lossless (trivia preserved) and total (always produces a tiling
//! stream; problems are recorded in `errors`).

use crate::error::LexError;
use crate::symbols::keyword_from_str;
use crate::token::{Span, Token, TokenKind};

/// The output of lexing: the full (lossless) token stream plus any diagnostics.
/// `tokens` always tiles the source, even when `errors` is non-empty.
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub errors: Vec<LexError>,
}

/// Lex `source` into a lossless token stream. Never fails; see `LexResult`.
pub fn lex(source: &str) -> LexResult {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    tokens: Vec<Token>,
    errors: Vec<LexError>,
}

// ── Cursor primitives ───────────────────────────────────────────────────────

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Lexer<'a> {
        Lexer {
            src,
            pos: 0,
            tokens: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn rest(&self) -> &'a str {
        self.src.get(self.pos..).unwrap_or("")
    }

    /// Look ahead `n` chars without consuming. All call sites use `n <= 2`
    /// (maximal-munch and prefix detection), so this is O(1) in practice; the
    /// `rest()` slice is O(1) and at most three chars are decoded.
    fn nth(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    /// Consume one char, advancing by its UTF-8 length.
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    /// If the next char is `c`, consume it and return true.
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn eat_while<F: Fn(char) -> bool>(&mut self, pred: F) {
        while let Some(c) = self.peek() {
            if pred(c) {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }
}

// ── Driver ──────────────────────────────────────────────────────────────────

impl<'a> Lexer<'a> {
    fn run(mut self) -> LexResult {
        while !self.at_end() {
            let lo = self.pos;
            let kind = self.scan_one(lo);
            self.push(kind, lo);
        }
        self.push(TokenKind::Eof, self.pos);
        LexResult {
            tokens: self.tokens,
            errors: self.errors,
        }
    }

    fn push(&mut self, kind: TokenKind, lo: usize) {
        let span = Span { lo, hi: self.pos };
        let text = self.src.get(lo..self.pos).unwrap_or("").to_string();
        self.tokens.push(Token { kind, span, text });
    }

    fn error(&mut self, err: LexError) {
        self.errors.push(err);
    }

    /// Dispatch on the first character to the matching scanner.
    fn scan_one(&mut self, lo: usize) -> TokenKind {
        let c = match self.peek() {
            Some(c) => c,
            None => return TokenKind::Eof,
        };
        if c == '\n' {
            self.bump();
            return TokenKind::Newline;
        }
        if is_h_space(c) {
            self.eat_while(is_h_space);
            return TokenKind::Whitespace;
        }
        if c == '/' && matches!(self.nth(1), Some('/') | Some('*')) {
            return self.scan_comment(lo);
        }
        if c.is_ascii_digit() {
            return self.scan_number(lo);
        }
        if c == '"' {
            return self.scan_string(lo);
        }
        if c == 'r' && self.nth(1) == Some('"') {
            return self.scan_raw_string(lo);
        }
        if c == 'b' && self.nth(1) == Some('\'') {
            return self.scan_byte(lo);
        }
        // A leading `'` is a loop label (`'outer`) — Axiom has no char literals
        // or lifetimes, so `'` is unambiguous. (Byte literals `b'..'` are handled
        // above, where the `'` follows `b`.)
        if c == '\'' {
            return self.scan_label(lo);
        }
        if is_ident_start(c) {
            return self.scan_ident();
        }
        self.scan_punct(c, lo)
    }
}

// ── Comments (§2.2) ───────────────────────────────────────────────────────────

impl<'a> Lexer<'a> {
    fn scan_comment(&mut self, lo: usize) -> TokenKind {
        if self.nth(1) == Some('*') {
            self.scan_block_comment(lo)
        } else {
            self.scan_line_comment()
        }
    }

    /// `//` line or `///` doc (exactly three slashes). Eats to end of line.
    fn scan_line_comment(&mut self) -> TokenKind {
        let mut slashes = 0;
        while self.eat('/') {
            slashes += 1;
        }
        self.eat_while(|c| c != '\n');
        if slashes == 3 {
            TokenKind::DocComment
        } else {
            TokenKind::LineComment
        }
    }

    /// `/* ... */`, nestable. Records an error if unterminated.
    fn scan_block_comment(&mut self, lo: usize) -> TokenKind {
        self.bump(); // '/'
        self.bump(); // '*'
        let mut depth = 1u32;
        while depth > 0 {
            match self.bump() {
                None => {
                    self.error(LexError::UnterminatedBlockComment {
                        span: self.span_from(lo),
                    });
                    break;
                }
                Some('/') if self.eat('*') => depth += 1,
                Some('*') if self.eat('/') => depth -= 1,
                Some(_) => {}
            }
        }
        TokenKind::BlockComment
    }
}

// ── Identifiers & keywords (§2.3, §2.4) ───────────────────────────────────────

impl<'a> Lexer<'a> {
    fn scan_ident(&mut self) -> TokenKind {
        let lo = self.pos;
        self.eat_while(is_ident_continue);
        let word = self.src.get(lo..self.pos).unwrap_or("");
        match keyword_from_str(word) {
            Some(kw) => TokenKind::Keyword(kw),
            None => TokenKind::Ident,
        }
    }
}

// ── Labels (§7.1) ─────────────────────────────────────────────────────────────

impl<'a> Lexer<'a> {
    /// `'name` — a loop label. A lone `'` not followed by an identifier start is
    /// an unexpected character (recorded, lexed as `Unknown`, still tiles).
    fn scan_label(&mut self, lo: usize) -> TokenKind {
        self.bump(); // opening quote
        if self.peek().is_some_and(is_ident_start) {
            self.eat_while(is_ident_continue);
            TokenKind::Label
        } else {
            self.error(LexError::UnexpectedChar {
                span: self.span_from(lo),
            });
            TokenKind::Unknown
        }
    }
}

// ── Numbers (§2.5, §2.6) ──────────────────────────────────────────────────────

impl<'a> Lexer<'a> {
    fn scan_number(&mut self, lo: usize) -> TokenKind {
        if self.peek() == Some('0') {
            if let Some(kind) = self.try_radix(lo) {
                return kind;
            }
        }
        self.scan_decimal(lo)
    }

    /// `0x`/`0o`/`0b` prefixed integers. Returns `None` if not a radix prefix.
    fn try_radix(&mut self, lo: usize) -> Option<TokenKind> {
        let radix = match self.nth(1) {
            Some('x' | 'X') => 16,
            Some('o' | 'O') => 8,
            Some('b' | 'B') => 2,
            _ => return None,
        };
        self.bump(); // '0'
        self.bump(); // radix letter
        let start = self.pos;
        self.eat_while(|c| c == '_' || c.is_digit(radix));
        let digits: String = self.slice(start).chars().filter(|c| *c != '_').collect();
        match i64::from_str_radix(&digits, radix) {
            Ok(v) => Some(TokenKind::IntLit(v)),
            Err(_) => {
                self.error(LexError::InvalidNumber {
                    span: self.span_from(lo),
                });
                Some(TokenKind::IntLit(0))
            }
        }
    }

    fn scan_decimal(&mut self, lo: usize) -> TokenKind {
        self.eat_digits();
        let mut is_float = false;
        // A '.' starts a fraction only if a digit follows (so `1..5` and `1.foo`
        // are not consumed as floats).
        if self.peek() == Some('.') && self.nth(1).is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            self.bump();
            self.eat_digits();
        }
        if matches!(self.peek(), Some('e' | 'E')) && self.exponent_follows() {
            is_float = true;
            self.consume_exponent();
        }
        self.finish_number(lo, is_float)
    }

    fn eat_digits(&mut self) {
        self.eat_while(|c| c.is_ascii_digit() || c == '_');
    }

    fn exponent_follows(&self) -> bool {
        match self.nth(1) {
            Some(c) if c.is_ascii_digit() => true,
            Some('+' | '-') => self.nth(2).is_some_and(|c| c.is_ascii_digit()),
            _ => false,
        }
    }

    fn consume_exponent(&mut self) {
        self.bump(); // 'e' / 'E'
        if matches!(self.peek(), Some('+' | '-')) {
            self.bump();
        }
        self.eat_digits();
    }

    fn finish_number(&mut self, lo: usize, is_float: bool) -> TokenKind {
        let cleaned: String = self.slice(lo).chars().filter(|c| *c != '_').collect();
        if is_float {
            match cleaned.parse::<f64>() {
                Ok(v) => TokenKind::FloatLit(v),
                Err(_) => self.number_error(lo, TokenKind::FloatLit(0.0)),
            }
        } else {
            match cleaned.parse::<i64>() {
                Ok(v) => TokenKind::IntLit(v),
                Err(_) => self.number_error(lo, TokenKind::IntLit(0)),
            }
        }
    }

    fn number_error(&mut self, lo: usize, fallback: TokenKind) -> TokenKind {
        self.error(LexError::InvalidNumber {
            span: self.span_from(lo),
        });
        fallback
    }
}

// ── Strings & bytes (§2.5) ────────────────────────────────────────────────────

impl<'a> Lexer<'a> {
    fn scan_string(&mut self, lo: usize) -> TokenKind {
        self.bump(); // opening quote
        let mut value = String::new();
        loop {
            match self.peek() {
                None => {
                    self.error(LexError::UnterminatedString {
                        span: self.span_from(lo),
                    });
                    break;
                }
                Some('"') => {
                    self.bump();
                    break;
                }
                Some('\\') => self.scan_escape(&mut value),
                Some(c) => {
                    self.bump();
                    value.push(c);
                }
            }
        }
        TokenKind::StrLit(value)
    }

    /// Decode one escape into `out`. Records `InvalidEscape` on an unknown one.
    fn scan_escape(&mut self, out: &mut String) {
        let esc_lo = self.pos;
        self.bump(); // backslash
        match self.bump() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('0') => out.push('\0'),
            Some('u') => self.scan_unicode_escape(out, esc_lo),
            _ => self.invalid_escape(esc_lo, out),
        }
    }

    /// `\u{XXXX}` — hex code point in braces.
    fn scan_unicode_escape(&mut self, out: &mut String, esc_lo: usize) {
        if !self.eat('{') {
            self.invalid_escape(esc_lo, out);
            return;
        }
        self.eat_while(|c| c.is_ascii_hexdigit());
        let hex_start = esc_lo + "\\u{".len();
        let hex = self.src.get(hex_start..self.pos).unwrap_or("").to_string();
        let closed = self.eat('}');
        let decoded = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32);
        match (closed, decoded) {
            (true, Some(ch)) => out.push(ch),
            _ => self.invalid_escape(esc_lo, out),
        }
    }

    /// Record an invalid escape and keep the decode lossless: push the raw
    /// consumed source text (`\` + whatever followed) into the value, so the
    /// decoded string still reconstructs the source intent rather than silently
    /// dropping bytes. Callers must have advanced `pos` past the bad escape.
    fn invalid_escape(&mut self, esc_lo: usize, out: &mut String) {
        self.error(LexError::InvalidEscape {
            span: self.span_from(esc_lo),
        });
        out.push_str(self.slice(esc_lo));
    }

    fn scan_raw_string(&mut self, lo: usize) -> TokenKind {
        self.bump(); // 'r'
        self.bump(); // opening quote
        let start = self.pos;
        let mut closed = false;
        while let Some(c) = self.peek() {
            if c == '"' {
                closed = true;
                break;
            }
            self.bump();
        }
        let content = self.slice(start).to_string();
        if closed {
            self.bump();
        } else {
            self.error(LexError::UnterminatedString {
                span: self.span_from(lo),
            });
        }
        TokenKind::StrLit(content)
    }

    fn scan_byte(&mut self, lo: usize) -> TokenKind {
        self.bump(); // 'b'
        self.bump(); // opening quote
        let value = match self.peek() {
            // Empty `b''`: don't consume the closing quote as the value.
            Some('\'') => {
                self.error(LexError::EmptyByte {
                    span: self.span_from(lo),
                });
                0
            }
            Some('\\') => self.scan_byte_escape(lo),
            Some(c) if c.is_ascii() && !c.is_ascii_control() => {
                self.bump();
                c as u8
            }
            _ => {
                self.error(LexError::UnterminatedByte {
                    span: self.span_from(lo),
                });
                0
            }
        };
        if !self.eat('\'') {
            self.error(LexError::UnterminatedByte {
                span: self.span_from(lo),
            });
        }
        TokenKind::ByteLit(value)
    }

    fn scan_byte_escape(&mut self, lo: usize) -> u8 {
        self.bump(); // backslash
        match self.bump() {
            Some('n') => b'\n',
            Some('t') => b'\t',
            Some('r') => b'\r',
            Some('\\') => b'\\',
            Some('\'') => b'\'',
            Some('0') => 0,
            _ => {
                self.error(LexError::InvalidEscape {
                    span: self.span_from(lo),
                });
                0
            }
        }
    }
}

// ── Operators & punctuation (§2.7) ────────────────────────────────────────────

impl<'a> Lexer<'a> {
    fn scan_punct(&mut self, c: char, lo: usize) -> TokenKind {
        use crate::token::Punct;
        if let Some(p) = simple_delim(c) {
            self.bump();
            return TokenKind::Punct(p);
        }
        let p = match c {
            ':' => self.either(':', Punct::ColonColon, Punct::Colon),
            '.' => self.scan_dot(),
            '+' => self.either('=', Punct::PlusEq, Punct::Plus),
            '-' => self.scan_minus(),
            '*' => self.either('=', Punct::StarEq, Punct::Star),
            // A `/` only reaches here when it is NOT `//`/`/*` (that is decided in
            // `scan_one`), so it is always the division operator or `/=`.
            '/' => self.either('=', Punct::SlashEq, Punct::Slash),
            '%' => self.either('=', Punct::PercentEq, Punct::Percent),
            '!' => self.either('=', Punct::Ne, Punct::Bang),
            '&' => self.either('&', Punct::AmpAmp, Punct::Amp),
            '|' => self.either('|', Punct::PipePipe, Punct::Pipe),
            '=' => self.scan_eq(),
            '<' => self.scan_angle('<', Punct::Le, Punct::Shl, Punct::Lt),
            '>' => self.scan_angle('>', Punct::Ge, Punct::Shr, Punct::Gt),
            _ => return self.scan_unknown(lo),
        };
        TokenKind::Punct(p)
    }

    /// Consume the first char; if `second` follows, consume it and return
    /// `with`, else return `plain`.
    fn either(
        &mut self,
        second: char,
        with: crate::token::Punct,
        plain: crate::token::Punct,
    ) -> crate::token::Punct {
        self.bump();
        if self.eat(second) {
            with
        } else {
            plain
        }
    }

    fn scan_dot(&mut self) -> crate::token::Punct {
        use crate::token::Punct;
        self.bump();
        if self.eat('.') {
            if self.eat('=') {
                Punct::DotDotEq
            } else {
                Punct::DotDot
            }
        } else {
            Punct::Dot
        }
    }

    fn scan_minus(&mut self) -> crate::token::Punct {
        use crate::token::Punct;
        self.bump();
        if self.eat('=') {
            Punct::MinusEq
        } else if self.eat('>') {
            Punct::Arrow
        } else {
            Punct::Minus
        }
    }

    fn scan_eq(&mut self) -> crate::token::Punct {
        use crate::token::Punct;
        self.bump();
        if self.eat('=') {
            Punct::EqEq
        } else if self.eat('>') {
            Punct::FatArrow
        } else {
            Punct::Eq
        }
    }

    /// `<`/`>`: doubled form (`<<`/`>>`), `=` form (`<=`/`>=`), or bare.
    fn scan_angle(
        &mut self,
        same: char,
        with_eq: crate::token::Punct,
        doubled: crate::token::Punct,
        plain: crate::token::Punct,
    ) -> crate::token::Punct {
        self.bump();
        if self.eat('=') {
            with_eq
        } else if self.eat(same) {
            doubled
        } else {
            plain
        }
    }

    fn scan_unknown(&mut self, lo: usize) -> TokenKind {
        self.bump();
        self.error(LexError::UnexpectedChar {
            span: self.span_from(lo),
        });
        TokenKind::Unknown
    }
}

// ── Small shared helpers ──────────────────────────────────────────────────────

impl<'a> Lexer<'a> {
    fn slice(&self, from: usize) -> &'a str {
        self.src.get(from..self.pos).unwrap_or("")
    }

    fn span_from(&self, lo: usize) -> Span {
        Span { lo, hi: self.pos }
    }
}

fn simple_delim(c: char) -> Option<crate::token::Punct> {
    use crate::token::Punct;
    match c {
        '(' => Some(Punct::LParen),
        ')' => Some(Punct::RParen),
        '[' => Some(Punct::LBracket),
        ']' => Some(Punct::RBracket),
        '{' => Some(Punct::LBrace),
        '}' => Some(Punct::RBrace),
        ',' => Some(Punct::Comma),
        ';' => Some(Punct::Semicolon),
        '^' => Some(Punct::Caret),
        '?' => Some(Punct::Question),
        _ => None,
    }
}

fn is_h_space(c: char) -> bool {
    // '\r' is whitespace (LF-only line terminator, §2.1); U+FEFF (BOM / ZWNBSP)
    // is treated as whitespace so a BOM-prefixed file lexes cleanly rather than
    // producing an Unknown token.
    c == ' ' || c == '\t' || c == '\r' || c == '\u{feff}'
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
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
}
