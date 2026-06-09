//! Lowering: CST/AST views → HIR nodes.
//!
//! Two-pass design (per `docs/hir-testing.md` §4):
//!   Pass 1 — collect item definitions into a symbol table.
//!   Pass 2 — resolve names in bodies against the symbol table and lexical scopes.
//!
//! This module handles the **structural** lowering (CST shape → HIR shape).
//! Name resolution lives in `resolve.rs`.

mod block;
mod catch_else;
mod error;
mod expr;
mod item;
mod pattern;
mod ty;

use crate::hir_types::*;
use crate::HirDiagnostic;
use lexer::Span;
use parser::ast::{self, AstNode};

pub(super) use catch_else::{lower_catch_expr, lower_else_expr};

/// Structural lowering only — produces HIR items and defs without name resolution.
/// Takes a `start_id` so DefIds are globally unique across modules.
/// Returns (items, defs, diagnostics, next_id).
pub fn lower_structural(
    root: &ast::SourceFile,
    source: &str,
    start_id: usize,
) -> (Vec<Item>, Vec<Def>, Vec<HirDiagnostic>, usize) {
    let mut ctx = LowerCtx::new(source);
    ctx.next_id = start_id;
    for item_node in root.items() {
        let item = match item::lower_item(item_node, &mut ctx) {
            Some(i) => i,
            None => continue,
        };
        ctx.items.push(item);
    }
    let next_id = ctx.next_id;
    (ctx.items, ctx.defs, ctx.diagnostics, next_id)
}

pub struct LowerCtx {
    #[allow(dead_code)]
    pub source: String,
    pub next_id: usize,
    pub items: Vec<Item>,
    pub diagnostics: Vec<HirDiagnostic>,
    pub defs: Vec<Def>,
}

#[derive(Debug, Clone)]
pub struct Def {
    pub name: String,
    pub def_id: DefId,
    pub kind: DefKind,
    pub visibility: Visibility,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    Fn,
    Struct,
    Enum,
    Trait,
    Variant,
    Field,
    Param,
    TypeParam,
    Local,
    Builtin,
    ErrorSet,
    ErrorVariant,
}

impl LowerCtx {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            next_id: 0,
            items: Vec::new(),
            diagnostics: Vec::new(),
            defs: Vec::new(),
        }
    }

    pub fn alloc_id(&mut self) -> HirId {
        let id = self.next_id;
        self.next_id += 1;
        HirId(id)
    }

    pub(crate) fn span_of(&self, node: &parser::SyntaxNode) -> Span {
        let s = node.span();
        Span { lo: s.lo, hi: s.hi }
    }

    pub(crate) fn diag(&mut self, diag: HirDiagnostic) {
        self.diagnostics.push(diag);
    }
}

fn name_text(name: &ast::Name) -> String {
    name.text().unwrap_or_default()
}

fn token_text(token: Option<parser::SyntaxToken>) -> String {
    token.map(|t| t.text().to_string()).unwrap_or_default()
}

fn path_last_segment(path: Option<ast::Path>) -> String {
    path.and_then(|p| {
        p.segments()
            .into_iter()
            .last()
            .and_then(|seg| seg.name_token())
    })
    .map(|t| t.text().to_string())
    .unwrap_or_default()
}

fn path_last_segment_from_node(node: &parser::SyntaxNode) -> Option<String> {
    ast::PathExpr::cast(node.clone())
        .and_then(|pe| pe.path())
        .map(|p| path_last_segment(Some(p)))
}

/// The qualifier of a path expression: every segment *before* the last, joined
/// by `::`. `List::new` → `Some("List")`, `a::b::c` → `Some("a::b")`, a bare
/// `new` → `None`. Used to resolve associated-function calls.
fn path_qualifier_from_node(node: &parser::SyntaxNode) -> Option<String> {
    let path = ast::PathExpr::cast(node.clone()).and_then(|pe| pe.path())?;
    let names: Vec<String> = path
        .segments()
        .into_iter()
        .filter_map(|seg| seg.name_token())
        .map(|t| t.text().to_string())
        .collect();
    if names.len() < 2 {
        return None;
    }
    Some(names[..names.len() - 1].join("::"))
}

fn lit_kind_from_token(token: &parser::SyntaxToken) -> LitKind {
    use parser::SyntaxKind;
    match token.kind() {
        SyntaxKind::IntLit => {
            let text = token.text();
            text.parse::<i64>()
                .map(LitKind::Int)
                .unwrap_or(LitKind::Int(0))
        }
        SyntaxKind::FloatLit => {
            let text = token.text();
            text.parse::<f64>()
                .map(LitKind::Float)
                .unwrap_or(LitKind::Float(0.0))
        }
        SyntaxKind::StrLit => {
            let text = token.text();
            let inner = &text[1..text.len().saturating_sub(1)];
            LitKind::String(decode_str_escapes(inner))
        }
        SyntaxKind::KwTrue => LitKind::Bool(true),
        SyntaxKind::KwFalse => LitKind::Bool(false),
        _ => LitKind::Unit,
    }
}

/// Decode the escape sequences a string literal may contain — `\n`, `\t`, `\r`,
/// `\0`, `\\`, `\"` — into the actual characters for the runtime string value.
/// The lexer has already validated escapes, so an unrecognized one keeps its
/// trailing character verbatim rather than erroring again.
fn decode_str_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}
