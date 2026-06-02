//! Expression grammar — a Pratt (precedence-climbing) parser implementing the
//! §2.7 precedence table. `expr_bp` is the climbing loop; `lhs` handles prefix
//! operators and primaries; `postfix` handles the tightest forms (call, field,
//! method, index, cast). Binding powers come from `infix_bp` — the table is the
//! spec.
//!
//! Two lexer gaps are noted at the crate level and surface here: there is no `?`
//! token (Option postfix, spec §6.5) and `>>` is one `Shr` (nested generics).

use super::stmt::block;
use super::ty::ty;
use super::{path, pattern};
use crate::parser::{CompletedMarker, Parser};
use crate::syntax_kind::SyntaxKind as K;

/// Tokens that can begin an expression (first-set), for optional-expr decisions
/// (`return`, `break`).
const EXPR_START: &[K] = &[
    K::IntLit,
    K::FloatLit,
    K::ByteLit,
    K::StrLit,
    K::KwTrue,
    K::KwFalse,
    K::Ident,
    K::KwSelf,
    K::KwSelfType,
    K::LParen,
    K::LBracket,
    K::LBrace,
    K::KwIf,
    K::KwMatch,
    K::KwLoop,
    K::KwScope,
    K::Pipe,
    K::PipePipe,
    K::Minus,
    K::Bang,
    K::KwTry,
    K::KwReturn,
    K::KwBreak,
    K::KwContinue,
];

/// Binding power above every infix operator — used for prefix operands so unary
/// binds tighter than any binary operator.
const PREFIX_BP: u8 = 24;

/// Parse an expression, including a trailing low-precedence `catch` handler.
pub(super) fn expr(p: &mut Parser) {
    let Some(cm) = expr_bp(p, 0) else {
        return;
    };
    if p.at(K::KwCatch) {
        let m = cm.precede(p);
        p.bump(); // catch
        expr_bp(p, 0); // handler (commonly a `|e| ...` closure)
        m.complete(p, K::CatchExpr);
    }
}

/// The precedence-climbing core: parse an expression whose operators bind at
/// least `min_bp`.
fn expr_bp(p: &mut Parser, min_bp: u8) -> Option<CompletedMarker> {
    let mut lhs = lhs(p)?;
    while let Some((l_bp, r_bp, kind)) = infix_bp(p.current()) {
        if l_bp < min_bp {
            break;
        }
        let m = lhs.precede(p);
        p.bump(); // the operator
        expr_bp(p, r_bp);
        lhs = m.complete(p, kind);
    }
    Some(lhs)
}

/// Infix operator binding powers `(left, right, node-kind)`. Left-associative
/// operators use `(2L, 2L+1)`; assignment is right-associative (`(3, 2)`).
fn infix_bp(op: K) -> Option<(u8, u8, K)> {
    let bp = match op {
        K::Eq | K::PlusEq | K::MinusEq | K::StarEq | K::SlashEq | K::PercentEq => {
            return Some((3, 2, K::AssignExpr))
        }
        K::DotDot | K::DotDotEq => return Some((4, 5, K::RangeExpr)),
        K::PipePipe => (6, 7),
        K::AmpAmp => (8, 9),
        K::EqEq | K::Ne | K::Lt | K::Le | K::Gt | K::Ge => (10, 11),
        K::Pipe => (12, 13),
        K::Caret => (14, 15),
        K::Amp => (16, 17),
        K::Shl | K::Shr => (18, 19),
        K::Plus | K::Minus => (20, 21),
        K::Star | K::Slash | K::Percent => (22, 23),
        _ => return None,
    };
    Some((bp.0, bp.1, K::BinExpr))
}

/// Prefix operators and primaries, then trailing postfix. This is the single
/// recursion chokepoint, so the stack-overflow guard lives here: past the depth
/// limit it recovers (consumes a token, reports) instead of descending.
fn lhs(p: &mut Parser) -> Option<CompletedMarker> {
    if !p.enter_recursion() {
        p.err_and_bump("expression nesting too deep");
        p.leave_recursion();
        return None;
    }
    let result = lhs_inner(p);
    p.leave_recursion();
    result
}

fn lhs_inner(p: &mut Parser) -> Option<CompletedMarker> {
    match p.current() {
        K::Minus | K::Bang => Some(prefix(p, K::PrefixExpr)),
        K::KwTry => Some(prefix(p, K::TryExpr)),
        _ => {
            let cm = primary(p)?;
            Some(postfix(p, cm))
        }
    }
}

/// A prefix-operator expression: consume the operator, then an operand binding
/// tighter than any infix.
fn prefix(p: &mut Parser, kind: K) -> CompletedMarker {
    let m = p.start();
    p.bump();
    expr_bp(p, PREFIX_BP);
    m.complete(p, kind)
}

/// The tightest forms, applied left to right: call, field/method, index, cast.
fn postfix(p: &mut Parser, mut lhs: CompletedMarker) -> CompletedMarker {
    loop {
        lhs = match p.current() {
            K::LParen => call(p, lhs),
            K::Dot => field_or_method(p, lhs),
            K::LBracket => index(p, lhs),
            K::KwAs => cast(p, lhs),
            _ => break,
        };
    }
    lhs
}

fn primary(p: &mut Parser) -> Option<CompletedMarker> {
    match p.current() {
        K::IntLit | K::FloatLit | K::ByteLit | K::StrLit | K::KwTrue | K::KwFalse => {
            Some(literal(p))
        }
        K::Ident | K::KwSelf | K::KwSelfType => Some(path_or_struct_lit(p)),
        K::LParen => Some(paren_expr(p)),
        K::LBracket => Some(list_lit(p)),
        K::LBrace => Some(block(p)),
        K::KwIf => Some(if_expr(p)),
        K::KwMatch => Some(match_expr(p)),
        K::KwLoop => Some(loop_expr(p)),
        K::KwScope => Some(scope_expr(p)),
        K::Pipe | K::PipePipe => Some(closure_expr(p)),
        K::KwReturn => Some(jump_expr(p, K::ReturnStmt, true)),
        K::KwBreak => Some(jump_expr(p, K::BreakStmt, true)),
        K::KwContinue => Some(jump_expr(p, K::ContinueStmt, false)),
        _ => {
            p.err_and_bump("expected an expression");
            None
        }
    }
}

fn literal(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump();
    m.complete(p, K::LiteralExpr)
}

/// A path expression, or a struct literal `Path { fields }` when allowed.
fn path_or_struct_lit(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    path(p, false);
    if !p.no_struct() && p.at(K::LBrace) {
        struct_lit_fields(p);
        return m.complete(p, K::StructLitExpr);
    }
    m.complete(p, K::PathExpr)
}

/// `{ field (, field)* }` where field is `name: expr`, shorthand `name`, or
/// `..base`.
fn struct_lit_fields(p: &mut Parser) {
    let m = p.start();
    p.bump(); // {
    while !p.at(K::RBrace) && !p.at_end() {
        if p.eat(K::DotDot) {
            expr(p); // ..base
            break;
        }
        struct_lit_field(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBrace);
    m.complete(p, K::StructLitFieldList);
}

fn struct_lit_field(p: &mut Parser) {
    let m = p.start();
    p.expect(K::Ident);
    if p.eat(K::Colon) {
        expr(p);
    }
    m.complete(p, K::StructLitField);
}

fn paren_expr(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(); // (
    if !p.eat(K::RParen) {
        expr(p);
        p.expect(K::RParen);
    }
    m.complete(p, K::ParenExpr)
}

fn list_lit(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(); // [
    while !p.at(K::RBracket) && !p.at_end() {
        expr(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBracket);
    m.complete(p, K::ListLitExpr)
}

/// `if cond { .. } (else if .. | else { .. })?`. The condition forbids struct
/// literals so `if x { }` reads `x` then the block.
fn if_expr(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(); // if
    cond(p);
    block(p);
    if p.eat(K::KwElse) {
        if p.at(K::KwIf) {
            if_expr(p);
        } else {
            block(p);
        }
    }
    m.complete(p, K::IfExpr)
}

/// Parse a condition/scrutinee expression with struct literals disabled.
fn cond(p: &mut Parser) {
    let prev = p.set_no_struct(true);
    expr(p);
    p.set_no_struct(prev);
}

fn match_expr(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(); // match
    cond(p);
    match_arm_list(p);
    m.complete(p, K::MatchExpr)
}

fn match_arm_list(p: &mut Parser) {
    let m = p.start();
    p.expect(K::LBrace);
    while !p.at(K::RBrace) && !p.at_end() {
        match_arm(p);
    }
    p.expect(K::RBrace);
    m.complete(p, K::MatchArmList);
}

fn match_arm(p: &mut Parser) {
    let m = p.start();
    pattern::pattern(p);
    if p.at(K::KwIf) {
        let g = p.start();
        p.bump();
        expr(p);
        g.complete(p, K::MatchGuard);
    }
    p.expect(K::FatArrow);
    expr(p);
    p.eat(K::Comma);
    m.complete(p, K::MatchArm);
}

/// The three `loop` forms (§7.1): infinite, pre-condition, iterator.
fn loop_expr(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(); // loop
    if p.eat(K::KwIf) {
        cond(p);
    } else if !p.at(K::LBrace) {
        pattern::pattern(p);
        p.expect(K::KwIn);
        cond(p);
    }
    block(p);
    m.complete(p, K::LoopExpr)
}

/// `scope |s| { .. }` (§9.2).
fn scope_expr(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(); // scope
    closure_params(p);
    block(p);
    m.complete(p, K::ScopeExpr)
}

/// `|a, b| body` or `|| body`, with optional `-> Type` before the body.
fn closure_expr(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    closure_params(p);
    if p.eat(K::Arrow) {
        ty(p);
    }
    expr(p);
    m.complete(p, K::ClosureExpr)
}

fn closure_params(p: &mut Parser) {
    let m = p.start();
    if p.eat(K::PipePipe) {
        m.complete(p, K::ClosureParamList);
        return;
    }
    p.expect(K::Pipe);
    while !p.at(K::Pipe) && !p.at_end() {
        closure_param(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::Pipe);
    m.complete(p, K::ClosureParamList);
}

fn closure_param(p: &mut Parser) {
    let m = p.start();
    p.expect(K::Ident);
    if p.eat(K::Colon) {
        ty(p);
    }
    m.complete(p, K::ClosureParam);
}

/// `return [expr]`, `break [expr]`, `continue` — diverging expressions. When
/// `takes_value` and the next token can start an expression, an operand is
/// parsed.
fn jump_expr(p: &mut Parser, kind: K, takes_value: bool) -> CompletedMarker {
    let m = p.start();
    p.bump(); // return / break / continue
    if takes_value && p.at_any(EXPR_START) {
        expr(p);
    }
    m.complete(p, kind)
}

// ── postfix builders ────────────────────────────────────────────────────────

fn call(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    arg_list(p);
    m.complete(p, K::CallExpr)
}

fn arg_list(p: &mut Parser) {
    let m = p.start();
    p.bump(); // (
    while !p.at(K::RParen) && !p.at_end() {
        expr(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RParen);
    m.complete(p, K::ArgList);
}

fn field_or_method(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    p.bump(); // .
              // A member name is an identifier or a keyword used as a name (`s.spawn(..)`,
              // `x.match(..)` — keywords read as identifiers in member position, §9.2).
    if p.at(K::Ident) || p.current().is_keyword() {
        let n = p.start();
        p.bump();
        n.complete(p, K::NameRef);
    } else {
        p.error("expected a field or method name");
    }
    if p.at(K::LParen) {
        arg_list(p);
        m.complete(p, K::MethodCallExpr)
    } else {
        m.complete(p, K::FieldExpr)
    }
}

fn index(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    p.bump(); // [
    expr(p);
    p.expect(K::RBracket);
    m.complete(p, K::IndexExpr)
}

fn cast(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    p.bump(); // as
    ty(p);
    m.complete(p, K::CastExpr)
}
