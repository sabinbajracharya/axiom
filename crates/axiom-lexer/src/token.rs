//! Token data — `Span`, `TokenKind`, `Token`, and the `LineMap` positional
//! helper. Plain data only; naming lives in `symbols`, scanning in `lexer`.
//!
//! Positions follow `docs/lexer-testing.md` §2: a `Span` is a half-open byte
//! range `[lo, hi)`; human-facing `line:col` is *derived* from a byte offset via
//! `LineMap`, so byte offset is the single source of positional truth.

/// A half-open byte range into the source, `[lo, hi)`. `hi == lo` is a
/// zero-width span (used by `Eof`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub lo: usize,
    pub hi: usize,
}

/// Reserved words (§2.4). Recognition and display labels live in `symbols`;
/// this enum is just the closed set. `self` and `Self` are distinct keywords.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Keyword {
    Val,
    Var,
    Fn,
    Struct,
    Enum,
    Trait,
    Impl,
    Let,
    Inout,
    Sink,
    Match,
    If,
    Else,
    Loop,
    Break,
    Continue,
    Return,
    Try,
    Catch,
    Errdefer,
    Error,
    Panic,
    Mod,
    Use,
    Pub,
    Scope,
    Spawn,
    True,
    False,
    SelfValue,
    SelfType,
    As,
    In,
    Is,
}

impl Keyword {
    /// Every keyword variant — used by consistency tests and the recognition
    /// table. Adding a variant means adding it here too.
    pub const ALL: &'static [Keyword] = &[
        Keyword::Val,
        Keyword::Var,
        Keyword::Fn,
        Keyword::Struct,
        Keyword::Enum,
        Keyword::Trait,
        Keyword::Impl,
        Keyword::Let,
        Keyword::Inout,
        Keyword::Sink,
        Keyword::Match,
        Keyword::If,
        Keyword::Else,
        Keyword::Loop,
        Keyword::Break,
        Keyword::Continue,
        Keyword::Return,
        Keyword::Try,
        Keyword::Catch,
        Keyword::Errdefer,
        Keyword::Error,
        Keyword::Panic,
        Keyword::Mod,
        Keyword::Use,
        Keyword::Pub,
        Keyword::Scope,
        Keyword::Spawn,
        Keyword::True,
        Keyword::False,
        Keyword::SelfValue,
        Keyword::SelfType,
        Keyword::As,
        Keyword::In,
        Keyword::Is,
    ];
}

/// Operators and punctuation (§2.7). Grouped into one enum so `TokenKind` stays
/// small and the display-name match stays short and exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Punct {
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
    Colon,
    ColonColon,
    Arrow,
    FatArrow,
    Dot,
    DotDot,
    DotDotEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    AmpAmp,
    Pipe,
    PipePipe,
    Caret,
    Shl,
    Shr,
    Bang,
    Eq,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    Question,
}

impl Punct {
    /// Every punctuation variant — used by consistency tests.
    pub const ALL: &'static [Punct] = &[
        Punct::LParen,
        Punct::RParen,
        Punct::LBracket,
        Punct::RBracket,
        Punct::LBrace,
        Punct::RBrace,
        Punct::Comma,
        Punct::Semicolon,
        Punct::Colon,
        Punct::ColonColon,
        Punct::Arrow,
        Punct::FatArrow,
        Punct::Dot,
        Punct::DotDot,
        Punct::DotDotEq,
        Punct::Plus,
        Punct::Minus,
        Punct::Star,
        Punct::Slash,
        Punct::Percent,
        Punct::Amp,
        Punct::AmpAmp,
        Punct::Pipe,
        Punct::PipePipe,
        Punct::Caret,
        Punct::Shl,
        Punct::Shr,
        Punct::Bang,
        Punct::Eq,
        Punct::EqEq,
        Punct::Ne,
        Punct::Lt,
        Punct::Le,
        Punct::Gt,
        Punct::Ge,
        Punct::PlusEq,
        Punct::MinusEq,
        Punct::StarEq,
        Punct::SlashEq,
        Punct::PercentEq,
        Punct::Question,
    ];
}

/// What a token is. Literal variants carry their *decoded* value (so the
/// snapshot can show `value=` alongside the raw text). Trivia (whitespace,
/// comments, newlines) are real variants — lexing is lossless (§3).
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Trivia
    Whitespace,
    Newline,
    LineComment,
    DocComment,
    BlockComment,
    // Literals (decoded value)
    IntLit(i64),
    FloatLit(f64),
    ByteLit(u8),
    StrLit(String),
    // Names
    Ident,
    Keyword(Keyword),
    // A loop label, e.g. `'outer` (§7.1). Axiom has no char literals or
    // lifetimes, so a leading `'` is unambiguously a label.
    Label,
    // Operators / punctuation
    Punct(Punct),
    // An unexpected character; paired with a `LexError` so the stream still tiles.
    Unknown,
    // End of input (zero-width, always last).
    Eof,
}

impl TokenKind {
    /// True for whitespace, newlines, and comments — the tokens the parser
    /// filters out (the only place trivia is dropped; see §3).
    pub fn is_trivia(&self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace
                | TokenKind::Newline
                | TokenKind::LineComment
                | TokenKind::DocComment
                | TokenKind::BlockComment
        )
    }
}

/// A single lexed token: its kind, its byte span, and the exact source slice it
/// covers. Storing the text makes the stream losslessly reconstructable on its
/// own (`invariants::reconstruct`).
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}

/// Maps byte offsets to 1-based `(line, col)`. Built once per source; `col` is
/// counted in characters (Unicode scalars) from the start of the line.
///
/// **Line terminator: LF only** (DESIGN_SPEC §2.1 — LF is canonical, enforced by
/// the formatter). A `\r` is ordinary horizontal whitespace and does *not* start
/// a new line, so `\r\n` advances one line (via its `\n`) and a lone `\r`
/// (old-Mac style) advances none. This is a deliberate decision, not an
/// oversight: Axiom source is LF.
pub struct LineMap {
    /// Byte offset of the start of each line. Always begins with `0`.
    line_starts: Vec<usize>,
}

impl LineMap {
    pub fn new(source: &str) -> LineMap {
        let mut line_starts = vec![0];
        for (i, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineMap { line_starts }
    }

    /// Resolve a byte `offset` to a 1-based `(line, col)`.
    pub fn locate(&self, source: &str, offset: usize) -> (usize, usize) {
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(next) => next.saturating_sub(1),
        };
        let line_start = self.line_starts.get(line_idx).copied().unwrap_or(0);
        let col = source
            .get(line_start..offset)
            .map_or(0, |s| s.chars().count());
        (line_idx + 1, col + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_trivia_classifies_kinds() {
        assert!(TokenKind::Whitespace.is_trivia());
        assert!(TokenKind::Newline.is_trivia());
        assert!(TokenKind::BlockComment.is_trivia());
        assert!(!TokenKind::Ident.is_trivia());
        assert!(!TokenKind::Eof.is_trivia());
        assert!(!TokenKind::IntLit(1).is_trivia());
    }

    #[test]
    fn test_linemap_first_line() {
        let src = "let x";
        let map = LineMap::new(src);
        assert_eq!(map.locate(src, 0), (1, 1));
        assert_eq!(map.locate(src, 4), (1, 5));
    }

    #[test]
    fn test_linemap_second_line_after_newline() {
        let src = "ab\ncd";
        let map = LineMap::new(src);
        assert_eq!(map.locate(src, 3), (2, 1)); // 'c'
        assert_eq!(map.locate(src, 5), (2, 3)); // end of "cd"
    }

    #[test]
    fn test_linemap_counts_unicode_by_char() {
        let src = "é=1"; // 'é' is two bytes; col is char-based
        let map = LineMap::new(src);
        assert_eq!(map.locate(src, 2), (1, 2)); // '=' is the 2nd char
    }

    #[test]
    fn test_linemap_lone_cr_is_not_a_line_break() {
        // LF-only (§2.1): a bare '\r' does not start a new line.
        let src = "a\rb";
        let map = LineMap::new(src);
        assert_eq!(map.locate(src, 2), (1, 3)); // 'b' is still line 1, col 3
    }

    #[test]
    fn test_linemap_crlf_advances_one_line() {
        let src = "a\r\nb";
        let map = LineMap::new(src);
        assert_eq!(map.locate(src, 3), (2, 1)); // 'b' after CRLF is line 2
    }
}
