//! Lexing of integer and float literals (§2.5, §2.6).

use super::*;

// ── Numbers (§2.5, §2.6) ──────────────────────────────────────────────────────

impl<'a> Lexer<'a> {
    pub(super) fn scan_number(&mut self, lo: usize) -> TokenKind {
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
