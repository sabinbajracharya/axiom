//! `SyntaxKind` — the single flat enum naming **both** token kinds and node
//! kinds, and the single source of truth for their display labels
//! (`docs/parser-testing.md` §2.1, §8.3).
//!
//! The whole enum, its `ALL` list, its `label`, and the trivia/token/node group
//! predicates are generated from **one** list by the `syntax_kinds!` macro, so
//! they cannot drift: you cannot add a variant without it appearing everywhere.
//! Labels are the variant name itself (`stringify!`), so there are **no** label
//! string literals anywhere to get out of sync.
//!
//! The lexer's `TokenKind` crosses into the token half of this enum through the
//! single bridge `from_lexer` — the only coupling point between the two crates.

use axiom_lexer::{Keyword, Punct, TokenKind};

/// Generate `SyntaxKind` plus everything derived from it from one grouped list.
/// The three groups (`trivia`, `tokens`, `nodes`) drive the `is_trivia` /
/// `is_token` / `is_node` predicates; the variant name doubles as its label.
macro_rules! syntax_kinds {
    (
        trivia   { $($triv:ident),* $(,)? }
        tokens   { $($tok:ident),* $(,)? }
        keywords { $($kw:ident),* $(,)? }
        nodes    { $($node:ident),* $(,)? }
    ) => {
        /// A node or token kind. Token kinds (incl. trivia and keywords) tag tree
        /// leaves; node kinds tag interior nodes. One flat enum so the tree is
        /// homogeneous (rust-analyzer model).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum SyntaxKind {
            $($triv,)*
            $($tok,)*
            $($kw,)*
            $($node,)*
        }

        impl SyntaxKind {
            /// Every variant, for consistency tests and exhaustive iteration.
            pub const ALL: &'static [SyntaxKind] = &[
                $(SyntaxKind::$triv,)*
                $(SyntaxKind::$tok,)*
                $(SyntaxKind::$kw,)*
                $(SyntaxKind::$node,)*
            ];

            /// The canonical display label — the variant's own name. No label
            /// literals exist, so the serializer can never hardcode a wrong one.
            pub fn label(self) -> &'static str {
                match self {
                    $(SyntaxKind::$triv => stringify!($triv),)*
                    $(SyntaxKind::$tok => stringify!($tok),)*
                    $(SyntaxKind::$kw => stringify!($kw),)*
                    $(SyntaxKind::$node => stringify!($node),)*
                }
            }

            /// Whitespace / newline / comments — the leaves the parser ignores
            /// when deciding grammar, but still keeps in the tree (§3).
            pub fn is_trivia(self) -> bool {
                matches!(self, $(SyntaxKind::$triv)|*)
            }

            /// A reserved keyword token. Used to allow keywords in member-name
            /// position (`s.spawn(..)`, §9.2), where they read as identifiers.
            pub fn is_keyword(self) -> bool {
                matches!(self, $(SyntaxKind::$kw)|*)
            }

            /// True for any leaf kind (trivia, keyword, or other token), false
            /// for interior node kinds.
            pub fn is_token(self) -> bool {
                matches!(
                    self,
                    $(SyntaxKind::$triv)|* $(| SyntaxKind::$tok)* $(| SyntaxKind::$kw)*
                )
            }

            /// True for interior node kinds.
            pub fn is_node(self) -> bool {
                !self.is_token()
            }
        }
    };
}

syntax_kinds! {
    trivia {
        Whitespace,
        Newline,
        LineComment,
        DocComment,
        BlockComment,
    }
    tokens {
        // literals (decoded value lives on the token text/lexer side)
        IntLit,
        FloatLit,
        ByteLit,
        StrLit,
        // names
        Ident,
        // a loop label, e.g. `'outer` (§7.1)
        Label,
        // punctuation (one per lexer `Punct`)
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
        // an unexpected source character (paired with a diagnostic)
        Unknown,
        // end of input — never placed in the tree, but a valid kind to peek
        Eof,
    }
    keywords {
        // one per lexer `Keyword` (§2.4)
        KwVal,
        KwVar,
        KwFn,
        KwStruct,
        KwEnum,
        KwTrait,
        KwImpl,
        KwLet,
        KwInout,
        KwSink,
        KwMatch,
        KwIf,
        KwElse,
        KwLoop,
        KwBreak,
        KwContinue,
        KwReturn,
        KwTry,
        KwCatch,
        KwErrdefer,
        KwError,
        KwPanic,
        KwMod,
        KwUse,
        KwPub,
        KwScope,
        KwSpawn,
        KwTrue,
        KwFalse,
        KwSelf,
        KwSelfType,
        KwSuper,
        KwCrate,
        KwAs,
        KwIn,
        KwIs,
        KwSubscript,
        KwYield,
    }
    nodes {
        // root
        SourceFile,
        // items
        FnDef,
        StructDef,
        EnumDef,
        TraitDef,
        ImplBlock,
        ModDef,
        UseDecl,
        ConstDef,
        ErrorSetDef,
        // item parts
        Visibility,
        ParamList,
        Param,
        SelfParam,
        FieldList,
        Field,
        VariantList,
        Variant,
        VariantPayload,
        GenericParamList,
        GenericParam,
        TraitBounds,
        RetType,
        UseTree,
        UseGroup,
        UseRename,
        ErrorVariantList,
        ErrorVariant,
        AssocItemList,
        TraitItemList,
        SubscriptDef,
        YieldStmt,
        // statements
        LetStmt,
        ExprStmt,
        ReturnStmt,
        BreakStmt,
        ContinueStmt,
        ErrdeferStmt,
        // expressions
        BlockExpr,
        LiteralExpr,
        PathExpr,
        BinExpr,
        PrefixExpr,
        CallExpr,
        MethodCallExpr,
        FieldExpr,
        IndexExpr,
        ParenExpr,
        IfExpr,
        MatchExpr,
        LoopExpr,
        ClosureExpr,
        StructLitExpr,
        CastExpr,
        RangeExpr,
        TryExpr,
        AssignExpr,
        CatchExpr,
        ScopeExpr,
        SpawnExpr,
        ListLitExpr,
        ArgList,
        // paths / names
        Path,
        PathSegment,
        Name,
        NameRef,
        // loop forms
        LoopCondition,
        LoopIter,
        LoopLabel,
        // match
        MatchArmList,
        MatchArm,
        MatchGuard,
        // closures
        ClosureParamList,
        ClosureParam,
        // struct literal parts
        StructLitFieldList,
        StructLitField,
        // patterns
        WildcardPat,
        LiteralPat,
        IdentPat,
        TupleStructPat,
        StructPat,
        PathPat,
        OrPat,
        RestPat,
        RangePat,
        StructPatFieldList,
        StructPatField,
        TuplePatFieldList,
        // types
        PathType,
        GenericArgList,
        ErrorUnionType,
        ErrorSetUnionType,
        DynType,
        UnitType,
        FnType,
        FnTypeParams,
        // recovery
        Error,
    }
}

impl SyntaxKind {
    /// The single bridge from the lexer's token classification into this enum.
    /// Every `TokenKind` maps to exactly one token `SyntaxKind`; a consistency
    /// test asserts the result is always `is_token`.
    pub fn from_lexer(kind: &TokenKind) -> SyntaxKind {
        match kind {
            TokenKind::Whitespace => SyntaxKind::Whitespace,
            TokenKind::Newline => SyntaxKind::Newline,
            TokenKind::LineComment => SyntaxKind::LineComment,
            TokenKind::DocComment => SyntaxKind::DocComment,
            TokenKind::BlockComment => SyntaxKind::BlockComment,
            TokenKind::IntLit(_) => SyntaxKind::IntLit,
            TokenKind::FloatLit(_) => SyntaxKind::FloatLit,
            TokenKind::ByteLit(_) => SyntaxKind::ByteLit,
            TokenKind::StrLit(_) => SyntaxKind::StrLit,
            TokenKind::Ident => SyntaxKind::Ident,
            TokenKind::Label => SyntaxKind::Label,
            TokenKind::Keyword(kw) => keyword_kind(*kw),
            TokenKind::Punct(p) => punct_kind(*p),
            TokenKind::Unknown => SyntaxKind::Unknown,
            TokenKind::Eof => SyntaxKind::Eof,
        }
    }
}

/// Map a lexer keyword to its token kind. Exhaustive: a new `Keyword` variant
/// fails to compile here until it is mapped.
fn keyword_kind(kw: Keyword) -> SyntaxKind {
    match kw {
        Keyword::Val => SyntaxKind::KwVal,
        Keyword::Var => SyntaxKind::KwVar,
        Keyword::Fn => SyntaxKind::KwFn,
        Keyword::Struct => SyntaxKind::KwStruct,
        Keyword::Enum => SyntaxKind::KwEnum,
        Keyword::Trait => SyntaxKind::KwTrait,
        Keyword::Impl => SyntaxKind::KwImpl,
        Keyword::Let => SyntaxKind::KwLet,
        Keyword::Inout => SyntaxKind::KwInout,
        Keyword::Sink => SyntaxKind::KwSink,
        Keyword::Match => SyntaxKind::KwMatch,
        Keyword::If => SyntaxKind::KwIf,
        Keyword::Else => SyntaxKind::KwElse,
        Keyword::Loop => SyntaxKind::KwLoop,
        Keyword::Break => SyntaxKind::KwBreak,
        Keyword::Continue => SyntaxKind::KwContinue,
        Keyword::Return => SyntaxKind::KwReturn,
        Keyword::Try => SyntaxKind::KwTry,
        Keyword::Catch => SyntaxKind::KwCatch,
        Keyword::Errdefer => SyntaxKind::KwErrdefer,
        Keyword::Error => SyntaxKind::KwError,
        Keyword::Panic => SyntaxKind::KwPanic,
        Keyword::Mod => SyntaxKind::KwMod,
        Keyword::Use => SyntaxKind::KwUse,
        Keyword::Pub => SyntaxKind::KwPub,
        Keyword::Scope => SyntaxKind::KwScope,
        Keyword::Spawn => SyntaxKind::KwSpawn,
        Keyword::True => SyntaxKind::KwTrue,
        Keyword::False => SyntaxKind::KwFalse,
        Keyword::SelfValue => SyntaxKind::KwSelf,
        Keyword::SelfType => SyntaxKind::KwSelfType,
        Keyword::As => SyntaxKind::KwAs,
        Keyword::In => SyntaxKind::KwIn,
        Keyword::Is => SyntaxKind::KwIs,
        Keyword::Subscript => SyntaxKind::KwSubscript,
        Keyword::Yield => SyntaxKind::KwYield,
        Keyword::Super => SyntaxKind::KwSuper,
        Keyword::Crate => SyntaxKind::KwCrate,
    }
}

/// Map a lexer punctuation token to its kind. Exhaustive for the same reason.
fn punct_kind(p: Punct) -> SyntaxKind {
    match p {
        Punct::LParen => SyntaxKind::LParen,
        Punct::RParen => SyntaxKind::RParen,
        Punct::LBracket => SyntaxKind::LBracket,
        Punct::RBracket => SyntaxKind::RBracket,
        Punct::LBrace => SyntaxKind::LBrace,
        Punct::RBrace => SyntaxKind::RBrace,
        Punct::Comma => SyntaxKind::Comma,
        Punct::Semicolon => SyntaxKind::Semicolon,
        Punct::Colon => SyntaxKind::Colon,
        Punct::ColonColon => SyntaxKind::ColonColon,
        Punct::Arrow => SyntaxKind::Arrow,
        Punct::FatArrow => SyntaxKind::FatArrow,
        Punct::Dot => SyntaxKind::Dot,
        Punct::DotDot => SyntaxKind::DotDot,
        Punct::DotDotEq => SyntaxKind::DotDotEq,
        Punct::Plus => SyntaxKind::Plus,
        Punct::Minus => SyntaxKind::Minus,
        Punct::Star => SyntaxKind::Star,
        Punct::Slash => SyntaxKind::Slash,
        Punct::Percent => SyntaxKind::Percent,
        Punct::Amp => SyntaxKind::Amp,
        Punct::AmpAmp => SyntaxKind::AmpAmp,
        Punct::Pipe => SyntaxKind::Pipe,
        Punct::PipePipe => SyntaxKind::PipePipe,
        Punct::Caret => SyntaxKind::Caret,
        Punct::Shl => SyntaxKind::Shl,
        Punct::Shr => SyntaxKind::Shr,
        Punct::Bang => SyntaxKind::Bang,
        Punct::Eq => SyntaxKind::Eq,
        Punct::EqEq => SyntaxKind::EqEq,
        Punct::Ne => SyntaxKind::Ne,
        Punct::Lt => SyntaxKind::Lt,
        Punct::Le => SyntaxKind::Le,
        Punct::Gt => SyntaxKind::Gt,
        Punct::Ge => SyntaxKind::Ge,
        Punct::PlusEq => SyntaxKind::PlusEq,
        Punct::MinusEq => SyntaxKind::MinusEq,
        Punct::StarEq => SyntaxKind::StarEq,
        Punct::SlashEq => SyntaxKind::SlashEq,
        Punct::PercentEq => SyntaxKind::PercentEq,
        Punct::Question => SyntaxKind::Question,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_labels_unique_and_nonempty() {
        let mut seen = HashSet::new();
        for kind in SyntaxKind::ALL {
            let label = kind.label();
            assert!(!label.is_empty(), "empty label for {kind:?}");
            assert!(seen.insert(label), "duplicate label {label:?}");
        }
    }

    #[test]
    fn test_label_is_variant_name() {
        assert_eq!(SyntaxKind::FnDef.label(), "FnDef");
        assert_eq!(SyntaxKind::KwLet.label(), "KwLet");
        assert_eq!(SyntaxKind::IntLit.label(), "IntLit");
    }

    #[test]
    fn test_group_predicates_partition_the_enum() {
        for kind in SyntaxKind::ALL {
            // Every kind is either a token or a node, never both, never neither.
            assert_ne!(kind.is_token(), kind.is_node(), "ambiguous group {kind:?}");
            // Trivia is a strict subset of tokens.
            if kind.is_trivia() {
                assert!(kind.is_token(), "trivia {kind:?} must be a token");
            }
        }
    }

    #[test]
    fn test_from_lexer_always_yields_a_token_kind() {
        // The bridge must never produce a node kind. Cover every lexer TokenKind.
        let samples = [
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
            TokenKind::Unknown,
            TokenKind::Eof,
        ];
        for tk in &samples {
            assert!(SyntaxKind::from_lexer(tk).is_token());
        }
        for kw in Keyword::ALL {
            assert!(SyntaxKind::from_lexer(&TokenKind::Keyword(*kw)).is_token());
        }
        for p in Punct::ALL {
            assert!(SyntaxKind::from_lexer(&TokenKind::Punct(*p)).is_token());
        }
    }

    #[test]
    fn test_trivia_bridge_matches_lexer_is_trivia() {
        // A token is trivia on the parser side iff it is trivia on the lexer side.
        let trivia = [
            TokenKind::Whitespace,
            TokenKind::Newline,
            TokenKind::LineComment,
            TokenKind::DocComment,
            TokenKind::BlockComment,
        ];
        for tk in &trivia {
            assert!(tk.is_trivia());
            assert!(SyntaxKind::from_lexer(tk).is_trivia());
        }
        assert!(!SyntaxKind::from_lexer(&TokenKind::Ident).is_trivia());
    }
}
