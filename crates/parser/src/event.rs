//! Parser **events** and tree materialization (`docs/parser-testing.md` §8.2).
//!
//! The grammar never touches `Rc`/offsets/trivia. It emits a flat `Vec<Event>`
//! over the *significant* tokens; `build_tree` then walks the events alongside
//! the *full* lexer token list and produces the green tree, re-inserting trivia
//! by source position (§3). Decoupling the grammar from tree-building keeps the
//! one subtle thing — trivia attachment — in exactly one place.

use crate::green::{GreenNode, GreenNodeBuilder};
use crate::syntax_kind::SyntaxKind;
use lexer::{Token, TokenKind};
use std::rc::Rc;

/// A flat instruction in the parse stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Open a node. `forward_parent` (an absolute event index) lets a completed
    /// node be retroactively wrapped by a later one — the mechanism behind
    /// left-associative precedence (see `parser::CompletedMarker::precede`).
    Start {
        kind: SyntaxKind,
        forward_parent: Option<usize>,
    },
    /// Close the most recently opened node.
    Finish,
    /// Consume `len` bytes of the current source token as a leaf of kind `kind`.
    /// For an ordinary bump, `len` is the whole token. A token can be consumed in
    /// pieces by several `Token` events — the mechanism behind splitting `>>`
    /// into two `>` to close nested generics (`parser::Parser::split_one_gt`).
    Token { kind: SyntaxKind, len: usize },
    /// An abandoned `Start` (or a `Start` consumed as a forward parent). Skipped.
    Tombstone,
}

/// Materialize a green tree from the event stream and the full token list.
/// Trivia tokens (and never `Eof`) are inserted as leaves at their source
/// positions; significant tokens are consumed in lock-step with `Token` events.
pub fn build_tree(mut events: Vec<Event>, tokens: &[Token]) -> Rc<GreenNode> {
    let mut builder = GreenNodeBuilder::new();
    let mut cursor = TokenCursor::new(tokens);
    let mut open = 0usize; // currently-open node count

    for i in 0..events.len() {
        match std::mem::replace(&mut events[i], Event::Tombstone) {
            Event::Start {
                kind,
                forward_parent,
            } => {
                let kinds = gather_forward_parents(kind, forward_parent, &mut events);
                // Forward parents are outermost-last in `kinds`; open them
                // outer-to-inner.
                for kind in kinds.into_iter().rev() {
                    builder.start_node(kind);
                    open += 1;
                }
            }
            Event::Finish => {
                // Trailing trivia (after the last significant token) attaches to
                // the root, flushed just before the root closes.
                if open == 1 {
                    cursor.flush_trivia(&mut builder);
                }
                builder.finish_node();
                open = open.saturating_sub(1);
            }
            Event::Token { kind, len } => {
                cursor.flush_trivia(&mut builder);
                cursor.emit(&mut builder, kind, len);
            }
            Event::Tombstone => {}
        }
    }
    builder.finish()
}

/// A cursor over the full lexer token list with an intra-token byte offset, so a
/// single source token can be emitted as several leaves (token splitting).
struct TokenCursor<'a> {
    tokens: &'a [Token],
    index: usize,
    intra: usize,
}

impl<'a> TokenCursor<'a> {
    fn new(tokens: &'a [Token]) -> TokenCursor<'a> {
        TokenCursor {
            tokens,
            index: 0,
            intra: 0,
        }
    }

    /// Emit pending trivia leaves until the next significant token (or `Eof`).
    /// Only meaningful at a token boundary (`intra == 0`); trivia never appears
    /// mid-token.
    fn flush_trivia(&mut self, builder: &mut GreenNodeBuilder) {
        if self.intra != 0 {
            return;
        }
        while let Some(tok) = self.tokens.get(self.index) {
            if tok.kind == TokenKind::Eof || !tok.kind.is_trivia() {
                break;
            }
            builder.token(SyntaxKind::from_lexer(&tok.kind), tok.text.clone());
            self.index += 1;
        }
    }

    /// Emit `len` bytes of the current token as a leaf of kind `kind`, advancing
    /// the intra-token offset and moving to the next token when this one is
    /// exhausted. `Eof` is never placed in the tree.
    fn emit(&mut self, builder: &mut GreenNodeBuilder, kind: SyntaxKind, len: usize) {
        let Some(tok) = self.tokens.get(self.index) else {
            return;
        };
        if tok.kind == TokenKind::Eof {
            return;
        }
        let start = self.intra;
        let end = (start + len).min(tok.text.len());
        builder.token(kind, tok.text.get(start..end).unwrap_or("").to_string());
        self.intra = end;
        if self.intra >= tok.text.len() {
            self.index += 1;
            self.intra = 0;
        }
    }
}

/// Follow the `forward_parent` chain from a `Start`, marking each consumed
/// parent as a `Tombstone`. Returns the kinds innermost-first (so the caller
/// opens them in reverse).
fn gather_forward_parents(
    kind: SyntaxKind,
    mut forward_parent: Option<usize>,
    events: &mut [Event],
) -> Vec<SyntaxKind> {
    let mut kinds = vec![kind];
    while let Some(idx) = forward_parent {
        match std::mem::replace(&mut events[idx], Event::Tombstone) {
            Event::Start {
                kind: parent_kind,
                forward_parent: next,
            } => {
                kinds.push(parent_kind);
                forward_parent = next;
            }
            _ => forward_parent = None,
        }
    }
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invariants::check_all;
    use crate::syntax::SyntaxNode;

    /// `a + b` as a left-assoc BinExpr wrapping two LiteralExprs, wrapped in a
    /// SourceFile root — hand-written exactly as the Pratt loop would emit it
    /// (LiteralExpr(a) preceded by BinExpr). Forward-parent indices are absolute,
    /// so they account for the SourceFile `Start` at index 0.
    fn wrapped_bin_expr_events() -> Vec<Event> {
        vec![
            // 0
            Event::Start {
                kind: SyntaxKind::SourceFile,
                forward_parent: None,
            },
            // 1: lhs `a`, retroactively wrapped by the BinExpr at index 4
            Event::Start {
                kind: SyntaxKind::LiteralExpr,
                forward_parent: Some(4),
            },
            Event::Token {
                kind: SyntaxKind::Ident,
                len: 1,
            }, // 2: a
            Event::Finish, // 3
            // 4: the wrapping BinExpr
            Event::Start {
                kind: SyntaxKind::BinExpr,
                forward_parent: None,
            },
            Event::Token {
                kind: SyntaxKind::Plus,
                len: 1,
            }, // 5: +
            Event::Start {
                kind: SyntaxKind::LiteralExpr,
                forward_parent: None,
            }, // 6
            Event::Token {
                kind: SyntaxKind::Ident,
                len: 1,
            }, // 7: b
            Event::Finish, // 8: close rhs LiteralExpr
            Event::Finish, // 9: close BinExpr
            Event::Finish, // 10: close SourceFile
        ]
    }

    #[test]
    fn test_forward_parent_wraps_lhs() {
        let src = "a + b";
        let tokens = lexer::lex(src).tokens;
        let green = build_tree(wrapped_bin_expr_events(), &tokens);
        let root = SyntaxNode::new_root(green);

        // The outer node under SourceFile is the BinExpr, not the first lit.
        assert_eq!(root.child_nodes()[0].kind(), SyntaxKind::BinExpr);
        // And everything round-trips / tiles.
        assert_eq!(check_all(&root, src, &tokens), Ok(()));
    }

    #[test]
    fn test_trivia_only_input_attaches_to_root() {
        let src = "// just a comment\n";
        let tokens = lexer::lex(src).tokens;
        let events = vec![
            Event::Start {
                kind: SyntaxKind::SourceFile,
                forward_parent: None,
            },
            Event::Finish,
        ];
        let root = SyntaxNode::new_root(build_tree(events, &tokens));
        assert_eq!(check_all(&root, src, &tokens), Ok(()));
        // No significant children, but the comment is preserved as trivia.
        assert!(!root.tokens().is_empty());
    }

    #[test]
    fn test_leading_and_trailing_trivia_preserved() {
        let src = "  a + b  \n";
        let tokens = lexer::lex(src).tokens;
        let root = SyntaxNode::new_root(build_tree(wrapped_bin_expr_events(), &tokens));
        assert_eq!(check_all(&root, src, &tokens), Ok(()));
    }
}
