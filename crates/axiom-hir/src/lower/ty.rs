//! Type lowering: CST type nodes → HIR `HirTy`.

use super::path_last_segment;
use super::LowerCtx;
use crate::hir::*;
use crate::HirDiagnostic;
use axiom_parser::ast::{self, AstNode};
use axiom_parser::SyntaxKind;

pub(super) fn lower_ty(node: &axiom_parser::SyntaxNode, ctx: &mut LowerCtx) -> HirTy {
    if let Some(pt) = ast::PathType::cast(node.clone()) {
        let nr = pt
            .path()
            .map(|p| NameRef::unresolved(path_last_segment(Some(p))))
            .unwrap_or_else(|| NameRef::unresolved(""));
        let name_text = match &nr {
            NameRef::Resolved(r) => r.text.clone(),
            NameRef::Unresolved(u) => u.text.clone(),
        };
        if name_text == "()" {
            return HirTy::Unit;
        }
        // If generic args are present (`List<Int>`), produce HirTy::Instance.
        if let Some(generic_args) = pt.generic_arg_list() {
            let args = generic_args
                .args()
                .into_iter()
                .map(|arg_node| lower_ty(&arg_node, ctx))
                .collect();
            HirTy::Instance(InstanceTy { name: nr, args })
        } else {
            HirTy::Named(nr)
        }
    } else if let Some(slice) = ast::SliceType::cast(node.clone()) {
        // `[T]` → Slice(T). A missing element type (recovery) lowers to Error.
        let elem = slice
            .element_type()
            .map(|e| lower_ty(&e, ctx))
            .unwrap_or(HirTy::Error);
        HirTy::Slice(Box::new(elem))
    } else if let Some(_unit) = ast::UnitType::cast(node.clone()) {
        HirTy::Unit
    } else if let Some(_eu) = ast::ErrorUnionType::cast(node.clone()) {
        ctx.diag(HirDiagnostic::NotYetSupported {
            feature: "error union type".to_string(),
            span: ctx.span_of(node),
        });
        HirTy::Error
    } else if node.kind() == SyntaxKind::Error {
        HirTy::Error
    } else {
        ctx.diag(HirDiagnostic::NotYetSupported {
            feature: format!("type kind {:?}", node.kind()),
            span: ctx.span_of(node),
        });
        HirTy::Error
    }
}
