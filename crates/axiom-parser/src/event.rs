//! Parser **events** and tree materialization (`docs/parser-testing.md` §8.2).
//!
//! The grammar never touches `Rc`/offsets/trivia. It emits a flat `Vec<Event>`
//! over the *significant* tokens; `build_tree` then walks the events alongside
//! the *full* lexer token list and produces the green tree, re-inserting trivia
//! by source position (§3). Decoupling the grammar from tree-building keeps the
//! one subtle thing — trivia attachment — in exactly one place.

use crate::green::{GreenNode, GreenNodeBuilder};
use crate::syntax_kind::SyntaxKind;
use axiom_lexer::{Token, TokenKind};
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
    /// Consume the next significant token from the source stream.
    Token,
    /// An abandoned `Start` (or a `Start` consumed as a forward parent). Skipped.
    Tombstone,
}

/// Materialize a green tree from the event stream and the full token list.
/// Trivia tokens (and never `Eof`) are inserted as leaves at their source
/// positions; significant tokens are consumed in lock-step with `Token` events.
pub fn build_tree(mut events: Vec<Event>, tokens: &[Token]) -> Rc<GreenNode> {
    let mut builder = GreenNodeBuilder::new();
    let mut cursor = 0usize; // index into `tokens`
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
                    flush_trivia(&mut builder, tokens, &mut cursor);
                }
                builder.finish_node();
                open = open.saturating_sub(1);
            }
            Event::Token => {
                flush_trivia(&mut builder, tokens, &mut cursor);
                emit_significant(&mut builder, tokens, &mut cursor);
            }
            Event::Tombstone => {}
        }
    }
    builder.finish()
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

/// Emit pending trivia leaves until the next significant token (or `Eof`).
fn flush_trivia(builder: &mut GreenNodeBuilder, tokens: &[Token], cursor: &mut usize) {
    while let Some(tok) = tokens.get(*cursor) {
        if tok.kind == TokenKind::Eof || !tok.kind.is_trivia() {
            break;
        }
        builder.token(SyntaxKind::from_lexer(&tok.kind), tok.text.clone());
        *cursor += 1;
    }
}

/// Emit the next significant token leaf (trivia already flushed). `Eof` is never
/// placed in the tree.
fn emit_significant(builder: &mut GreenNodeBuilder, tokens: &[Token], cursor: &mut usize) {
    while let Some(tok) = tokens.get(*cursor) {
        if tok.kind == TokenKind::Eof {
            return;
        }
        if tok.kind.is_trivia() {
            // Defensive: should have been flushed already.
            builder.token(SyntaxKind::from_lexer(&tok.kind), tok.text.clone());
            *cursor += 1;
            continue;
        }
        builder.token(SyntaxKind::from_lexer(&tok.kind), tok.text.clone());
        *cursor += 1;
        return;
    }
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
            Event::Token,  // 2: a
            Event::Finish, // 3
            // 4: the wrapping BinExpr
            Event::Start {
                kind: SyntaxKind::BinExpr,
                forward_parent: None,
            },
            Event::Token, // 5: +
            Event::Start {
                kind: SyntaxKind::LiteralExpr,
                forward_parent: None,
            }, // 6
            Event::Token, // 7: b
            Event::Finish, // 8: close rhs LiteralExpr
            Event::Finish, // 9: close BinExpr
            Event::Finish, // 10: close SourceFile
        ]
    }

    #[test]
    fn test_forward_parent_wraps_lhs() {
        let src = "a + b";
        let tokens = axiom_lexer::lex(src).tokens;
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
        let tokens = axiom_lexer::lex(src).tokens;
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
        let tokens = axiom_lexer::lex(src).tokens;
        let root = SyntaxNode::new_root(build_tree(wrapped_bin_expr_events(), &tokens));
        assert_eq!(check_all(&root, src, &tokens), Ok(()));
    }
}
