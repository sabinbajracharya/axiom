//! Lowering: CST/AST views → HIR nodes.
//!
//! Two-pass design (per `docs/hir-testing.md` §4):
//!   Pass 1 — collect item definitions into a symbol table.
//!   Pass 2 — resolve names in bodies against the symbol table and lexical scopes.
//!
//! This module handles the **structural** lowering (CST shape → HIR shape).
//! Name resolution lives in `resolve.rs`.

mod block;
mod expr;
mod item;
mod pattern;
mod ty;

use crate::hir::*;
use crate::HirDiagnostic;
use axiom_lexer::Span;
use axiom_parser::ast::{self, AstNode};

pub fn lower(root: &ast::SourceFile, source: &str) -> Hir {
    let mut ctx = LowerCtx::new(source);
    for item_node in root.items() {
        let item = match item::lower_item(item_node, &mut ctx) {
            Some(i) => i,
            None => continue,
        };
        ctx.items.push(item);
    }
    crate::resolve::resolve(&mut ctx);
    Hir {
        items: ctx.items,
        diagnostics: ctx.diagnostics,
    }
}

pub(crate) struct LowerCtx {
    #[allow(dead_code)]
    pub source: String,
    pub next_id: usize,
    pub items: Vec<Item>,
    pub diagnostics: Vec<HirDiagnostic>,
    pub defs: Vec<Def>,
}

pub(crate) struct Def {
    pub name: String,
    pub def_id: DefId,
    pub kind: DefKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum DefKind {
    Fn,
    Struct,
    Enum,
    Variant,
    Field,
    Param,
    TypeParam,
    Local,
    Builtin,
}

impl LowerCtx {
    fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            next_id: 0,
            items: Vec::new(),
            diagnostics: Vec::new(),
            defs: Vec::new(),
        }
    }

    pub(crate) fn alloc_id(&mut self) -> HirId {
        let id = self.next_id;
        self.next_id += 1;
        HirId(id)
    }

    pub(crate) fn span_of(&self, node: &axiom_parser::SyntaxNode) -> Span {
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

fn token_text(token: Option<axiom_parser::SyntaxToken>) -> String {
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

fn path_last_segment_from_node(node: &axiom_parser::SyntaxNode) -> Option<String> {
    ast::PathExpr::cast(node.clone())
        .and_then(|pe| pe.path())
        .map(|p| path_last_segment(Some(p)))
}

fn lit_kind_from_token(token: &axiom_parser::SyntaxToken) -> LitKind {
    use axiom_parser::SyntaxKind;
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
            LitKind::String(inner.to_string())
        }
        SyntaxKind::KwTrue => LitKind::Bool(true),
        SyntaxKind::KwFalse => LitKind::Bool(false),
        _ => LitKind::Unit,
    }
}
