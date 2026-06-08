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
        K::LBracket => slice_type(p),
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

/// `[T]` — a slice: a runtime-sized, borrowed view of homogeneous elements.
/// The element type recurses, so `[[Int]]` and `[Pair<K, V>]` parse.
fn slice_type(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(); // [
    ty(p);
    p.expect(K::RBracket);
    m.complete(p, K::SliceType)
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

#[cfg(test)]
mod tests {
    // Tests legitimately panic/assert on failure. RUST_CONVENTIONS §3.4.
    #![allow(clippy::panic, clippy::expect_used)]
    use crate::{parse, serialize};

    /// Parse `src` as the annotated type of a parameter and return the tree dump.
    fn dump_param_ty(src: &str) -> String {
        let result = parse(&format!("fn f(x: {src}) {{ }}\n"));
        assert!(
            result.errors.is_empty(),
            "unexpected parse errors for `{src}`: {:?}",
            result.errors
        );
        serialize(&result.tree)
    }

    #[test]
    fn test_slice_type_parses() {
        let dump = dump_param_ty("[U8]");
        assert!(
            dump.contains("SliceType @"),
            "expected SliceType node:\n{dump}"
        );
    }

    #[test]
    fn test_slice_type_element_is_path() {
        // The element type `U8` parses as a normal PathType inside the slice.
        let dump = dump_param_ty("[U8]");
        let slice_pos = dump.find("SliceType @").expect("SliceType present");
        assert!(
            dump[slice_pos..].contains("PathType @"),
            "slice element should be a PathType:\n{dump}"
        );
    }

    #[test]
    fn test_nested_slice_type_parses() {
        let dump = dump_param_ty("[[Int]]");
        assert_eq!(
            dump.matches("SliceType @").count(),
            2,
            "expected two nested SliceType nodes:\n{dump}"
        );
    }
}
