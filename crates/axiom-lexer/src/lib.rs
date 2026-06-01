//! The Axiom lexer: source text → a lossless, tiling stream of `Token`s.
//!
//! Built test-first against [`docs/lexer-testing.md`](../../../docs/lexer-testing.md).
//! Lexing is **lossless** (whitespace and comments are real tokens) and **total**
//! (every input produces a stream that tiles the source; problems are reported in
//! `LexResult::errors`, never by failing). See `invariants` for the guarantees.
//!
//! ```
//! use axiom_lexer::{lex, serialize};
//! let result = lex("let x = 1");
//! assert!(result.errors.is_empty());
//! print!("{}", serialize(&result.tokens, "let x = 1"));
//! ```

mod error;
mod invariants;
mod lexer;
mod snapshot;
mod symbols;
mod token;

pub use error::LexError;
pub use invariants::{check_all, reconstruct, spans_match_text, tiles};
pub use lexer::{lex, LexResult};
pub use snapshot::serialize;
pub use symbols::{display_name, keyword_from_str};
pub use token::{Keyword, LineMap, Punct, Span, Token, TokenKind};
