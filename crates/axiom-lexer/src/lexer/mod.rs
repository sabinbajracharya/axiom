//! The scanner — the one deliberately stateful core (`docs/lexer-testing.md`
//! §5.1). A byte cursor walks the source; short single-purpose methods each scan
//! one token and return its `TokenKind`. Local mutation stays inside this struct.
//!
//! Lexing is lossless (trivia preserved) and total (always produces a tiling
//! stream; problems are recorded in `errors`).

use crate::error::LexError;
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

mod number;
mod punct;
mod string;
mod word;

#[cfg(test)]
mod tests;
