//! Pattern grammar (`DESIGN_SPEC.md` §7.2): wildcard, literal (incl. range),
//! identifier binding, path / tuple-struct / struct destructure, and `|`
//! alternatives. Guards (`if expr`) are attached at the match-arm level
//! (`expr.rs`), not here.

use super::path;
use crate::parser::{CompletedMarker, Parser};
use crate::syntax_kind::SyntaxKind as K;

const LITERAL_STARTS: &[K] = &[
    K::IntLit,
    K::FloatLit,
    K::ByteLit,
    K::StrLit,
    K::KwTrue,
    K::KwFalse,
    K::Minus,
];

const LITERAL_ATOMS: &[K] = &[
    K::IntLit,
    K::FloatLit,
    K::ByteLit,
    K::StrLit,
    K::KwTrue,
    K::KwFalse,
];

/// A full pattern, including top-level `|` alternatives.
pub(super) fn pattern(p: &mut Parser) {
    let lhs = pattern_single(p);
    if p.at(K::Pipe) {
        let m = lhs.precede(p);
        while p.eat(K::Pipe) {
            pattern_single(p);
        }
        m.complete(p, K::OrPat);
    }
}

fn pattern_single(p: &mut Parser) -> CompletedMarker {
    if p.at_contextual("_") {
        let m = p.start();
        p.bump();
        return m.complete(p, K::WildcardPat);
    }
    if p.at_any(LITERAL_STARTS) {
        return literal_pattern(p);
    }
    match p.current() {
        K::Ident | K::KwSelfType => path_pattern(p),
        _ => {
            let m = p.start();
            p.error("expected a pattern");
            if !p.at_end() {
                p.bump();
            }
            m.complete(p, K::Error)
        }
    }
}

/// A literal, or a literal range (`1..=9`).
fn literal_pattern(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    literal_atom(p);
    if p.at(K::DotDot) || p.at(K::DotDotEq) {
        p.bump();
        literal_atom(p);
        return m.complete(p, K::RangePat);
    }
    m.complete(p, K::LiteralPat)
}

/// Consume one literal token, allowing an optional leading `-`.
fn literal_atom(p: &mut Parser) {
    p.eat(K::Minus);
    if p.at_any(LITERAL_ATOMS) {
        p.bump();
    } else {
        p.error("expected a literal");
    }
}

/// `Name`, or a qualified path, optionally with a `(..)` or `{..}` payload.
/// A bare single identifier is a binding (`IdentPat`); a multi-segment path is a
/// `PathPat`.
fn path_pattern(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    let segments = path(p, true);
    match p.current() {
        K::LParen => {
            tuple_pat_fields(p);
            m.complete(p, K::TupleStructPat)
        }
        K::LBrace => {
            struct_pat_fields(p);
            m.complete(p, K::StructPat)
        }
        _ if segments == 1 => m.complete(p, K::IdentPat),
        _ => m.complete(p, K::PathPat),
    }
}

/// `( pattern (, pattern)* )`
fn tuple_pat_fields(p: &mut Parser) {
    let m = p.start();
    p.bump(); // (
    while !p.at(K::RParen) && !p.at_end() {
        pattern(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RParen);
    m.complete(p, K::TuplePatFieldList);
}

/// `{ field (, field)* }` where a field is `name` or `name: pattern` or `..`.
fn struct_pat_fields(p: &mut Parser) {
    let m = p.start();
    p.bump(); // {
    while !p.at(K::RBrace) && !p.at_end() {
        if p.eat(K::DotDot) {
            let r = p.start();
            r.complete(p, K::RestPat);
            break;
        }
        struct_pat_field(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBrace);
    m.complete(p, K::StructPatFieldList);
}

fn struct_pat_field(p: &mut Parser) {
    let m = p.start();
    p.expect(K::Ident);
    if p.eat(K::Colon) {
        pattern(p);
    }
    m.complete(p, K::StructPatField);
}
