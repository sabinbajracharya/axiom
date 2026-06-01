//! Lexer-stage errors. Lexing is *total* — it always produces a tiling token
//! stream (bad input becomes an `Unknown` token or a best-effort literal); these
//! errors are reported *alongside* the tokens, never in place of them. That is
//! what lets the fuzzer assert the tiling invariant on every input.

use crate::token::Span;

/// A diagnosable problem found while lexing, with the span it occurred at.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LexError {
    #[error("unterminated string literal")]
    UnterminatedString { span: Span },

    #[error("unterminated block comment")]
    UnterminatedBlockComment { span: Span },

    #[error("unterminated byte literal")]
    UnterminatedByte { span: Span },

    #[error("invalid escape sequence")]
    InvalidEscape { span: Span },

    #[error("invalid number literal")]
    InvalidNumber { span: Span },

    #[error("unexpected character")]
    UnexpectedChar { span: Span },
}

impl LexError {
    /// The source span this error points at.
    pub fn span(&self) -> Span {
        match self {
            LexError::UnterminatedString { span }
            | LexError::UnterminatedBlockComment { span }
            | LexError::UnterminatedByte { span }
            | LexError::InvalidEscape { span }
            | LexError::InvalidNumber { span }
            | LexError::UnexpectedChar { span } => *span,
        }
    }
}
