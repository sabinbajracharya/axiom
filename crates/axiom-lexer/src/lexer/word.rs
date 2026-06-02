//! Lexing of comments, identifiers/keywords, and loop labels.

use super::*;
use crate::symbols::keyword_from_str;

// ── Comments (§2.2) ───────────────────────────────────────────────────────────

impl<'a> Lexer<'a> {
    pub(super) fn scan_comment(&mut self, lo: usize) -> TokenKind {
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
    pub(super) fn scan_ident(&mut self) -> TokenKind {
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
    pub(super) fn scan_label(&mut self, lo: usize) -> TokenKind {
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
