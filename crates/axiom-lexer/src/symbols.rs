//! The single source of truth for keyword spellings and token display names
//! (`docs/lexer-testing.md` §5.2). The lexer recognizes keywords via
//! `keyword_from_str`; the serializer names tokens via `display_name`. Nothing
//! else hardcodes a keyword spelling or a kind label.

use crate::token::{Keyword, Punct, TokenKind};

/// Keyword spelling → variant. The recognition table; `keyword_from_str` reads
/// it, and a consistency test cross-checks it against `Keyword::ALL`.
const KEYWORDS: &[(&str, Keyword)] = &[
    ("val", Keyword::Val),
    ("var", Keyword::Var),
    ("fn", Keyword::Fn),
    ("struct", Keyword::Struct),
    ("enum", Keyword::Enum),
    ("trait", Keyword::Trait),
    ("impl", Keyword::Impl),
    ("let", Keyword::Let),
    ("inout", Keyword::Inout),
    ("sink", Keyword::Sink),
    ("match", Keyword::Match),
    ("if", Keyword::If),
    ("else", Keyword::Else),
    ("loop", Keyword::Loop),
    ("break", Keyword::Break),
    ("continue", Keyword::Continue),
    ("return", Keyword::Return),
    ("try", Keyword::Try),
    ("catch", Keyword::Catch),
    ("errdefer", Keyword::Errdefer),
    ("error", Keyword::Error),
    ("panic", Keyword::Panic),
    ("mod", Keyword::Mod),
    ("use", Keyword::Use),
    ("pub", Keyword::Pub),
    ("scope", Keyword::Scope),
    ("spawn", Keyword::Spawn),
    ("true", Keyword::True),
    ("false", Keyword::False),
    ("self", Keyword::SelfValue),
    ("Self", Keyword::SelfType),
    ("super", Keyword::Super),
    ("crate", Keyword::Crate),
    ("as", Keyword::As),
    ("in", Keyword::In),
    ("is", Keyword::Is),
    ("subscript", Keyword::Subscript),
    ("yield", Keyword::Yield),
];

/// Recognize an identifier-shaped slice as a keyword, if it is one.
pub fn keyword_from_str(s: &str) -> Option<Keyword> {
    KEYWORDS
        .iter()
        .find(|(spelling, _)| *spelling == s)
        .map(|(_, kw)| *kw)
}

/// The canonical display name of a token kind (used by the snapshot serializer).
/// Value-carrying literals render only their label here; the value is appended
/// separately by the serializer.
pub fn display_name(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Whitespace => "Whitespace",
        TokenKind::Newline => "Newline",
        TokenKind::LineComment => "LineComment",
        TokenKind::DocComment => "DocComment",
        TokenKind::BlockComment => "BlockComment",
        TokenKind::IntLit(_) => "IntLit",
        TokenKind::FloatLit(_) => "FloatLit",
        TokenKind::ByteLit(_) => "ByteLit",
        TokenKind::StrLit(_) => "StrLit",
        TokenKind::Ident => "Ident",
        TokenKind::Label => "Label",
        TokenKind::Keyword(kw) => keyword_label(*kw),
        TokenKind::Punct(p) => punct_label(*p),
        TokenKind::Unknown => "Unknown",
        TokenKind::Eof => "Eof",
    }
}

/// Display label for a keyword (`Keyword::Let` → `"KwLet"`). Exhaustive: adding
/// a keyword without a label fails to compile — that is the §5.2 guarantee.
pub fn keyword_label(kw: Keyword) -> &'static str {
    match kw {
        Keyword::Val => "KwVal",
        Keyword::Var => "KwVar",
        Keyword::Fn => "KwFn",
        Keyword::Struct => "KwStruct",
        Keyword::Enum => "KwEnum",
        Keyword::Trait => "KwTrait",
        Keyword::Impl => "KwImpl",
        Keyword::Let => "KwLet",
        Keyword::Inout => "KwInout",
        Keyword::Sink => "KwSink",
        Keyword::Match => "KwMatch",
        Keyword::If => "KwIf",
        Keyword::Else => "KwElse",
        Keyword::Loop => "KwLoop",
        Keyword::Break => "KwBreak",
        Keyword::Continue => "KwContinue",
        Keyword::Return => "KwReturn",
        Keyword::Try => "KwTry",
        Keyword::Catch => "KwCatch",
        Keyword::Errdefer => "KwErrdefer",
        Keyword::Error => "KwError",
        Keyword::Panic => "KwPanic",
        Keyword::Mod => "KwMod",
        Keyword::Use => "KwUse",
        Keyword::Pub => "KwPub",
        Keyword::Scope => "KwScope",
        Keyword::Spawn => "KwSpawn",
        Keyword::True => "KwTrue",
        Keyword::False => "KwFalse",
        Keyword::SelfValue => "KwSelf",
        Keyword::SelfType => "KwSelfType",
        Keyword::As => "KwAs",
        Keyword::In => "KwIn",
        Keyword::Is => "KwIs",
        Keyword::Subscript => "KwSubscript",
        Keyword::Yield => "KwYield",
        Keyword::Super => "KwSuper",
        Keyword::Crate => "KwCrate",
    }
}

/// Display label for a punctuation token. Exhaustive for the same reason as
/// `keyword_label`.
pub fn punct_label(p: Punct) -> &'static str {
    match p {
        Punct::LParen => "LParen",
        Punct::RParen => "RParen",
        Punct::LBracket => "LBracket",
        Punct::RBracket => "RBracket",
        Punct::LBrace => "LBrace",
        Punct::RBrace => "RBrace",
        Punct::Comma => "Comma",
        Punct::Semicolon => "Semicolon",
        Punct::Colon => "Colon",
        Punct::ColonColon => "ColonColon",
        Punct::Arrow => "Arrow",
        Punct::FatArrow => "FatArrow",
        Punct::Dot => "Dot",
        Punct::DotDot => "DotDot",
        Punct::DotDotEq => "DotDotEq",
        Punct::Plus => "Plus",
        Punct::Minus => "Minus",
        Punct::Star => "Star",
        Punct::Slash => "Slash",
        Punct::Percent => "Percent",
        Punct::Amp => "Amp",
        Punct::AmpAmp => "AmpAmp",
        Punct::Pipe => "Pipe",
        Punct::PipePipe => "PipePipe",
        Punct::Caret => "Caret",
        Punct::Shl => "Shl",
        Punct::Shr => "Shr",
        Punct::Bang => "Bang",
        Punct::Eq => "Eq",
        Punct::EqEq => "EqEq",
        Punct::Ne => "Ne",
        Punct::Lt => "Lt",
        Punct::Le => "Le",
        Punct::Gt => "Gt",
        Punct::Ge => "Ge",
        Punct::PlusEq => "PlusEq",
        Punct::MinusEq => "MinusEq",
        Punct::StarEq => "StarEq",
        Punct::SlashEq => "SlashEq",
        Punct::PercentEq => "PercentEq",
        Punct::Question => "Question",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_keyword_table_covers_every_variant() {
        // Every Keyword::ALL variant must be recognizable from the table.
        for kw in Keyword::ALL {
            let found = KEYWORDS.iter().any(|(_, k)| k == kw);
            assert!(found, "keyword variant {kw:?} missing from KEYWORDS table");
        }
        assert_eq!(
            KEYWORDS.len(),
            Keyword::ALL.len(),
            "table/ALL length mismatch"
        );
    }

    #[test]
    fn test_keyword_spellings_unique_and_roundtrip() {
        let mut seen = HashSet::new();
        for (spelling, kw) in KEYWORDS {
            assert!(
                seen.insert(*spelling),
                "duplicate keyword spelling {spelling:?}"
            );
            assert_eq!(keyword_from_str(spelling), Some(*kw));
        }
    }

    #[test]
    fn test_keyword_labels_unique_and_prefixed() {
        let mut seen = HashSet::new();
        for kw in Keyword::ALL {
            let label = keyword_label(*kw);
            assert!(
                label.starts_with("Kw"),
                "keyword label {label:?} must start with Kw"
            );
            assert!(seen.insert(label), "duplicate keyword label {label:?}");
        }
    }

    #[test]
    fn test_punct_labels_unique() {
        let mut seen = HashSet::new();
        for p in Punct::ALL {
            assert!(
                seen.insert(punct_label(*p)),
                "duplicate punct label for {p:?}"
            );
        }
    }

    #[test]
    fn test_non_keywords_are_not_recognized() {
        assert_eq!(keyword_from_str("hello"), None);
        assert_eq!(keyword_from_str("Lett"), None);
        assert_eq!(keyword_from_str(""), None);
    }

    #[test]
    fn test_every_token_kind_has_a_unique_nonempty_label() {
        // §5.2: every TokenKind names exactly once, no orphans, no collisions.
        // The non-keyword/non-punct kinds (exhaustive — adding a variant here is
        // a compile reminder to register its label).
        let simple = [
            TokenKind::Whitespace,
            TokenKind::Newline,
            TokenKind::LineComment,
            TokenKind::DocComment,
            TokenKind::BlockComment,
            TokenKind::IntLit(0),
            TokenKind::FloatLit(0.0),
            TokenKind::ByteLit(0),
            TokenKind::StrLit(String::new()),
            TokenKind::Ident,
            TokenKind::Label,
            TokenKind::Unknown,
            TokenKind::Eof,
        ];
        let mut seen = HashSet::new();
        for kind in &simple {
            let label = display_name(kind);
            assert!(!label.is_empty(), "empty label for {kind:?}");
            assert!(seen.insert(label), "duplicate label {label:?}");
        }
        for kw in Keyword::ALL {
            let label = display_name(&TokenKind::Keyword(*kw));
            assert!(
                seen.insert(label),
                "keyword label {label:?} collides with another kind"
            );
        }
        for p in Punct::ALL {
            let label = display_name(&TokenKind::Punct(*p));
            assert!(
                seen.insert(label),
                "punct label {label:?} collides with another kind"
            );
        }
    }
}
