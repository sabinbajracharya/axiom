//! Lexing of string, raw-string, and byte literals (§2.5).

use super::*;

// ── Strings & bytes (§2.5) ────────────────────────────────────────────────────

impl<'a> Lexer<'a> {
    pub(super) fn scan_string(&mut self, lo: usize) -> TokenKind {
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

    pub(super) fn scan_raw_string(&mut self, lo: usize) -> TokenKind {
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

    pub(super) fn scan_byte(&mut self, lo: usize) -> TokenKind {
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
