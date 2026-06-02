//! Type-annotation grammar (`DESIGN_SPEC.md` §3, §6.2). Covers path types with
//! single-level generic arguments, the unit/paren type, and the error-union
//! sugar `E!T` plus parenthesized error-set unions `(A || B)`.
//!
//! Nested generics that close with `>>` (`Map<K, List<V>>`) parse correctly:
//! `>>` lexes as one `Shr`, which `Parser::eat_generic_close` splits into two
//! `>` so the inner and outer lists each claim one.

use super::path;
use crate::parser::{CompletedMarker, Parser};
use crate::syntax_kind::SyntaxKind as K;

/// Parse a type, returning the completed node. Always produces a node (an
/// `Error` node on unexpected input) so callers never juggle `Option`.
///
/// Types recurse (generic args, error-union `!`), so this shares the parser's
/// recursion-depth guard: past the limit it recovers instead of overflowing the
/// stack (totality, `docs/parser-testing.md` §5).
pub(super) fn ty(p: &mut Parser) -> CompletedMarker {
    if !p.enter_recursion() {
        let m = p.start();
        p.error("type nesting too deep");
        if !p.at_end() {
            p.bump();
        }
        p.leave_recursion();
        return m.complete(p, K::Error);
    }
    let result = ty_inner(p);
    p.leave_recursion();
    result
}

fn ty_inner(p: &mut Parser) -> CompletedMarker {
    let lhs = type_primary(p);
    // Error-union sugar: `ErrorSet ! SuccessType`.
    if p.at(K::Bang) {
        let m = lhs.precede(p);
        p.bump();
        ty(p);
        return m.complete(p, K::ErrorUnionType);
    }
    lhs
}

fn type_primary(p: &mut Parser) -> CompletedMarker {
    match p.current() {
        K::LParen => paren_type(p),
        K::Ident | K::KwSelfType => path_type(p),
        _ => {
            let m = p.start();
            // Recovery-set aware: a closer claimed by an enclosing construct is
            // left in place (empty `Error` node) so its owner can claim it,
            // rather than absorbed here.
            p.err_recover("expected a type");
            m.complete(p, K::Error)
        }
    }
}

/// `Name`, `Name::Seg`, optionally followed by `<args>`.
fn path_type(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    path(p, false);
    if p.at(K::Lt) {
        generic_arg_list(p);
    }
    m.complete(p, K::PathType)
}

/// `< Type (, Type)* >`. Nested generics work: a closing `>>` lexes as one
/// `Shr`, which `eat_generic_close` splits so the inner and outer lists each
/// claim one `>` (`Map<K, List<V>>`).
fn generic_arg_list(p: &mut Parser) {
    let m = p.start();
    p.bump(); // <
    while !p.at_generic_close() && !p.at_end() {
        ty(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.eat_generic_close();
    m.complete(p, K::GenericArgList);
}

/// `()` (unit), `(Type)` (grouping), or `(A || B)` (error-set union).
fn paren_type(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(); // (
    if p.eat(K::RParen) {
        return m.complete(p, K::UnitType);
    }
    ty(p);
    let mut is_union = false;
    while p.at(K::PipePipe) {
        is_union = true;
        p.bump();
        ty(p);
    }
    p.expect(K::RParen);
    m.complete(
        p,
        if is_union {
            K::ErrorSetUnionType
        } else {
            K::PathType
        },
    )
}
