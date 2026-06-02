//! Item grammar (`DESIGN_SPEC.md` §3, §8.1, §10): functions, structs, enums,
//! traits, impls, modules, `use`, error sets, and `const`.
//!
//! `item` starts the item node, consumes an optional `pub`, then dispatches on
//! the leading keyword and hands the open marker to the specific parser, which
//! completes it. Two spec words are **not** reserved keywords (§2.4) and so lex
//! as identifiers — `const` and the `for` of `impl Trait for Type` — handled
//! contextually here.

use super::stmt::block;
use super::ty::ty;
use crate::parser::{Marker, Parser};
use crate::syntax_kind::SyntaxKind as K;

/// Parse one top-level (or nested) item, recovering on anything unexpected.
pub(super) fn item(p: &mut Parser) {
    let m = p.start();
    opt_visibility(p);
    match p.current() {
        K::KwFn => fn_def(p, m),
        K::KwStruct => struct_def(p, m),
        K::KwEnum => enum_def(p, m),
        K::KwTrait => trait_def(p, m),
        K::KwImpl => impl_block(p, m),
        K::KwMod => mod_def(p, m),
        K::KwUse => use_decl(p, m),
        K::KwError => error_set_def(p, m),
        _ if p.at_contextual("const") => const_def(p, m),
        _ => {
            p.error("expected an item");
            if !p.at_end() {
                p.bump();
            }
            m.complete(p, K::Error);
        }
    }
}

fn opt_visibility(p: &mut Parser) {
    if p.at(K::KwPub) {
        let m = p.start();
        p.bump();
        m.complete(p, K::Visibility);
    }
}

fn name(p: &mut Parser) {
    let m = p.start();
    p.expect(K::Ident);
    m.complete(p, K::Name);
}

// ── functions ────────────────────────────────────────────────────────────────

/// `fn name<generics>(params) -> Ret { body }`. The body is optional (trait
/// method signatures end at `;`).
fn fn_def(p: &mut Parser, m: Marker) {
    p.bump(); // fn
    name(p);
    opt_generic_params(p);
    param_list(p);
    if p.eat(K::Arrow) {
        let r = p.start();
        ty(p);
        r.complete(p, K::RetType);
    }
    if p.at(K::LBrace) {
        block(p);
    } else {
        p.eat(K::Semicolon);
    }
    m.complete(p, K::FnDef);
}

/// `( param (, param)* )`.
fn param_list(p: &mut Parser) {
    let m = p.start();
    p.expect(K::LParen);
    while !p.at(K::RParen) && !p.at_end() {
        param(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RParen);
    m.complete(p, K::ParamList);
}

/// A parameter: optional convention (`let`/`inout`/`sink`), then either a `self`
/// receiver or `name: Type` (§4.2).
fn param(p: &mut Parser) {
    let m = p.start();
    p.eat_any(&[K::KwLet, K::KwInout, K::KwSink]);
    if p.at(K::KwSelf) {
        p.bump();
        m.complete(p, K::SelfParam);
        return;
    }
    p.expect(K::Ident);
    if p.eat(K::Colon) {
        ty(p);
    }
    m.complete(p, K::Param);
}

// ── generics ──────────────────────────────────────────────────────────────────

fn opt_generic_params(p: &mut Parser) {
    if p.at(K::Lt) {
        generic_params(p);
    }
}

/// `< Param (, Param)* >` where a param is `T` with optional `: Bound + Bound`.
fn generic_params(p: &mut Parser) {
    let m = p.start();
    p.bump(); // <
    while !p.at_generic_close() && !p.at_end() {
        generic_param(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.eat_generic_close();
    m.complete(p, K::GenericParamList);
}

fn generic_param(p: &mut Parser) {
    let m = p.start();
    p.expect(K::Ident);
    if p.eat(K::Colon) {
        trait_bounds(p);
    }
    m.complete(p, K::GenericParam);
}

fn trait_bounds(p: &mut Parser) {
    let m = p.start();
    ty(p);
    while p.eat(K::Plus) {
        ty(p);
    }
    m.complete(p, K::TraitBounds);
}

// ── structs / enums ──────────────────────────────────────────────────────────

fn struct_def(p: &mut Parser, m: Marker) {
    p.bump(); // struct
    name(p);
    opt_generic_params(p);
    if p.at(K::LBrace) {
        field_list(p);
    } else {
        p.eat(K::Semicolon);
    }
    m.complete(p, K::StructDef);
}

fn field_list(p: &mut Parser) {
    let m = p.start();
    p.bump(); // {
    while !p.at(K::RBrace) && !p.at_end() {
        field(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBrace);
    m.complete(p, K::FieldList);
}

fn field(p: &mut Parser) {
    let m = p.start();
    opt_visibility(p);
    p.expect(K::Ident);
    p.expect(K::Colon);
    ty(p);
    m.complete(p, K::Field);
}

fn enum_def(p: &mut Parser, m: Marker) {
    p.bump(); // enum
    name(p);
    opt_generic_params(p);
    variant_list(p);
    m.complete(p, K::EnumDef);
}

fn variant_list(p: &mut Parser) {
    let m = p.start();
    p.expect(K::LBrace);
    while !p.at(K::RBrace) && !p.at_end() {
        variant(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBrace);
    m.complete(p, K::VariantList);
}

/// `Name` or `Name(Type, Type)` (tuple payload, §3.4).
fn variant(p: &mut Parser) {
    let m = p.start();
    p.expect(K::Ident);
    if p.at(K::LParen) {
        variant_payload(p);
    }
    m.complete(p, K::Variant);
}

fn variant_payload(p: &mut Parser) {
    let m = p.start();
    p.bump(); // (
    while !p.at(K::RParen) && !p.at_end() {
        ty(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RParen);
    m.complete(p, K::VariantPayload);
}

// ── traits / impls ───────────────────────────────────────────────────────────

fn trait_def(p: &mut Parser, m: Marker) {
    p.bump(); // trait
    name(p);
    opt_generic_params(p);
    member_list(p, K::TraitItemList);
    m.complete(p, K::TraitDef);
}

/// `impl Type { .. }` or `impl Trait for Type { .. }` (§3.5). `for` is a
/// contextual identifier, not a keyword.
fn impl_block(p: &mut Parser, m: Marker) {
    p.bump(); // impl
    opt_generic_params(p);
    ty(p);
    if p.at_contextual("for") {
        p.bump();
        ty(p);
    }
    member_list(p, K::AssocItemList);
    m.complete(p, K::ImplBlock);
}

/// `{ method* }` shared by traits and impls. Each method may carry `pub`.
fn member_list(p: &mut Parser, kind: K) {
    let m = p.start();
    p.expect(K::LBrace);
    while !p.at(K::RBrace) && !p.at_end() {
        let before = p.pos();
        if p.at(K::KwFn) || p.at(K::KwPub) {
            let im = p.start();
            opt_visibility(p);
            if p.at(K::KwFn) {
                fn_def(p, im);
            } else {
                p.error("expected a method");
                im.complete(p, K::Error);
                if !p.at_end() {
                    p.bump();
                }
            }
        } else {
            p.err_recover("expected a method");
        }
        // `err_recover` may decline a closer claimed by an enclosing construct;
        // break on no progress so it bubbles out to its owner.
        if p.pos() == before {
            break;
        }
    }
    p.expect(K::RBrace);
    m.complete(p, kind);
}

// ── modules / use / error sets / const ───────────────────────────────────────

fn mod_def(p: &mut Parser, m: Marker) {
    p.bump(); // mod
    name(p);
    if p.at(K::LBrace) {
        p.bump();
        while !p.at(K::RBrace) && !p.at_end() {
            item(p);
        }
        p.expect(K::RBrace);
    } else {
        p.eat(K::Semicolon);
    }
    m.complete(p, K::ModDef);
}

fn use_decl(p: &mut Parser, m: Marker) {
    p.bump(); // use
    use_tree(p);
    p.eat(K::Semicolon);
    m.complete(p, K::UseDecl);
}

/// A path of segments, optionally ending in a `{group}` or `*` glob, with an
/// optional `as rename`. Mutually recursive with `use_group`, so it shares the
/// recursion-depth guard: deeply nested groups recover instead of overflowing
/// the stack (totality, `docs/parser-testing.md` §5).
fn use_tree(p: &mut Parser) {
    if !p.enter_recursion() {
        p.err_and_bump("use nesting too deep");
        p.leave_recursion();
        return;
    }
    use_tree_inner(p);
    p.leave_recursion();
}

fn use_tree_inner(p: &mut Parser) {
    let m = p.start();
    loop {
        if p.at(K::LBrace) {
            use_group(p);
            break;
        }
        if p.eat(K::Star) {
            break;
        }
        if !path_segment_token(p) {
            break;
        }
        if !p.eat(K::ColonColon) {
            break;
        }
    }
    if p.eat(K::KwAs) {
        let r = p.start();
        p.expect(K::Ident);
        r.complete(p, K::UseRename);
    }
    m.complete(p, K::UseTree);
}

fn path_segment_token(p: &mut Parser) -> bool {
    if p.at_any(&[K::Ident, K::KwSelf, K::KwSelfType]) {
        let s = p.start();
        p.bump();
        s.complete(p, K::PathSegment);
        true
    } else {
        false
    }
}

fn use_group(p: &mut Parser) {
    let m = p.start();
    p.bump(); // {
    while !p.at(K::RBrace) && !p.at_end() {
        use_tree(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBrace);
    m.complete(p, K::UseGroup);
}

fn error_set_def(p: &mut Parser, m: Marker) {
    p.bump(); // error
    name(p);
    error_variant_list(p);
    m.complete(p, K::ErrorSetDef);
}

fn error_variant_list(p: &mut Parser) {
    let m = p.start();
    p.expect(K::LBrace);
    while !p.at(K::RBrace) && !p.at_end() {
        let v = p.start();
        p.expect(K::Ident);
        v.complete(p, K::ErrorVariant);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBrace);
    m.complete(p, K::ErrorVariantList);
}

/// `const NAME: Type = expr` (§5.3). `const` is a contextual identifier.
fn const_def(p: &mut Parser, m: Marker) {
    p.bump(); // const (ident)
    name(p);
    if p.eat(K::Colon) {
        ty(p);
    }
    if p.eat(K::Eq) {
        super::expr::expr(p);
    }
    p.eat(K::Semicolon);
    m.complete(p, K::ConstDef);
}
