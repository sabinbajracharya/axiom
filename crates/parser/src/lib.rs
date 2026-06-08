//! The Axiom parser: a lossless **CST** (concrete syntax tree) over the lexer's
//! token stream.
//!
//! Built test-first against [`docs/parser-testing.md`](../../../docs/parser-testing.md).
//! Parsing is **total** (every input yields a tree + a diagnostics list, never a
//! panic or hang) and **lossless** (every token, trivia included, is a tree
//! leaf, so the tree reconstructs the source byte-for-byte). The tree is
//! rust-analyzer–shaped: an immutable green tree (`green`), a lazy red tree
//! (`syntax`), and typed views on top (`ast`).
//!
//! ```
//! use parser::{parse, serialize};
//! let result = parse("fn main() { }");
//! assert!(result.errors.is_empty());
//! print!("{}", serialize(&result.tree));
//! ```

pub mod ast;
mod error;
mod event;
mod grammar;
mod green;
mod invariants;
mod parser;
mod snapshot;
mod syntax;
mod syntax_kind;

pub use error::ParseError;
pub use green::{GreenNode, GreenNodeBuilder, GreenToken};
pub use invariants::{
    check_all, every_token_present, reconstruct, spans_well_formed, CoverageError, SpanError,
};
pub use snapshot::serialize;
pub use syntax::{SyntaxElement, SyntaxNode, SyntaxToken};
pub use syntax_kind::SyntaxKind;

use syntax::SyntaxNode as Node;

/// The result of a parse: the (always-present) tree and any diagnostics. Mirrors
/// the lexer's `LexResult` — problems live here, never in a failed `Result`.
pub struct ParseResult {
    pub tree: SyntaxNode,
    pub errors: Vec<ParseError>,
}

/// Parse Axiom source into a lossless CST. Lexes, runs the grammar to produce an
/// event stream, then materializes the green tree (re-inserting trivia) and
/// wraps it as a red root. Total: never panics, never hangs.
pub fn parse(source: &str) -> ParseResult {
    let lexed = lexer::lex(source);
    let mut p = parser::Parser::new(&lexed.tokens);
    grammar::source_file(&mut p);
    let (events, errors) = p.finish();
    let green = event::build_tree(events, &lexed.tokens);
    ParseResult {
        tree: Node::new_root(green),
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_is_lossless_and_well_formed() {
        let src = "fn main() {\n    val x = 1 + 2\n}\n";
        let result = parse(src);
        assert!(
            result.errors.is_empty(),
            "unexpected diagnostics: {:?}",
            result.errors
        );
        // The core guarantee: the tree rebuilds the source and tiles it.
        let tokens = lexer::lex(src).tokens;
        assert_eq!(check_all(&result.tree, src, &tokens), Ok(()));
    }

    #[test]
    fn test_parse_empty_input() {
        let result = parse("");
        assert!(result.errors.is_empty());
        assert_eq!(result.tree.kind(), SyntaxKind::SourceFile);
    }

    #[test]
    fn test_parse_recovers_from_garbage_without_panicking() {
        // Total + recovering: malformed input still yields a tree that tiles.
        let src = "fn @ } )) val";
        let result = parse(src);
        let tokens = lexer::lex(src).tokens;
        assert_eq!(check_all(&result.tree, src, &tokens), Ok(()));
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_top_level_garbage_run_collapses_to_one_error() {
        // A run of non-item tokens at file scope resyncs to the next item: ONE
        // diagnostic + ONE Error node for the run, then the `fn` parses cleanly.
        // (`%` is genuine garbage at item scope — unlike `@`, which now starts
        // an attribute.)
        let src = "% % % fn f() {}\n";
        let result = parse(src);
        let item_errs = result
            .errors
            .iter()
            .filter(|e| e.message.contains("expected an item"))
            .count();
        assert_eq!(
            item_errs, 1,
            "garbage run → one diagnostic: {:?}",
            result.errors
        );
        let dump = serialize(&result.tree);
        assert_eq!(
            dump.matches("Error @").count(),
            1,
            "garbage run should collapse to one Error node:\n{dump}"
        );
        assert!(
            dump.contains("FnDef @"),
            "the fn should still parse:\n{dump}"
        );
        // Losslessness still holds across the recovery.
        let tokens = lexer::lex(src).tokens;
        assert_eq!(check_all(&result.tree, src, &tokens), Ok(()));
    }
}
