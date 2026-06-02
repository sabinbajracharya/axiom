//! Grammar productions (`docs/parser-testing.md` §9). One module per production
//! family; this module owns the entry point (`source_file`) and the shared
//! `path` helper. Every function here is small and single-purpose, and every
//! loop either consumes a token or breaks — the termination discipline (§5).

mod expr;
mod item;
mod pattern;
mod stmt;
mod ty;

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind as K;

/// Parse a whole source file: a sequence of items until end of input.
pub(crate) fn source_file(p: &mut Parser) {
    let m = p.start();
    while !p.at_end() {
        item::item(p);
    }
    m.complete(p, K::SourceFile);
}

/// A `::`-separated path of name segments (e.g. `std::io::print`). When
/// `allow_dot` is set, `.` also separates segments — used by enum-variant
/// patterns (`FsError.NotFound`), where `.` is the spec's separator. Returns the
/// number of segments parsed.
pub(super) fn path(p: &mut Parser, allow_dot: bool) -> usize {
    let m = p.start();
    let mut segments = 0;
    loop {
        if !path_segment(p) {
            break;
        }
        segments += 1;
        let join = p.at(K::ColonColon) || (allow_dot && p.at(K::Dot));
        if join {
            p.bump();
        } else {
            break;
        }
    }
    m.complete(p, K::Path);
    segments
}

/// One path segment: an identifier, `self`, or `Self`. Returns whether one was
/// consumed.
fn path_segment(p: &mut Parser) -> bool {
    if p.at_any(&[K::Ident, K::KwSelf, K::KwSelfType]) {
        let m = p.start();
        p.bump();
        m.complete(p, K::PathSegment);
        true
    } else {
        false
    }
}
