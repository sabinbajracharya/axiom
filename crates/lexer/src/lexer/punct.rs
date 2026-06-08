//! Lexing of operators and punctuation, with maximal munch (§2.7).

use super::*;

// ── Operators & punctuation (§2.7) ────────────────────────────────────────────

impl<'a> Lexer<'a> {
    pub(super) fn scan_punct(&mut self, c: char, lo: usize) -> TokenKind {
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
