//! Type lowering: CST type nodes → HIR `HirTy`.

use super::path_last_segment;
use super::LowerCtx;
use crate::hir_types::*;
use crate::HirDiagnostic;
use parser::ast::{self, AstNode};
use parser::SyntaxKind;

pub(super) fn lower_ty(node: &parser::SyntaxNode, ctx: &mut LowerCtx) -> HirTy {
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
        // `E ! T` error-union sugar → desugared to `Result<T, E>` by the
        // resolver. Until then, lower as a placeholder instance.
        let error_type = _eu.error_type().map(|n| lower_ty(&n, ctx));
        let success_type = _eu.success_type().map(|n| lower_ty(&n, ctx));
        match (error_type, success_type) {
            (Some(e), Some(s)) => HirTy::Instance(InstanceTy {
                name: NameRef::unresolved("Result"),
                args: vec![s, e],
            }),
            _ => HirTy::Error,
        }
    } else if let Some(eu) = ast::ErrorSetUnionType::cast(node.clone()) {
        let members: Vec<HirTy> = eu
            .members()
            .into_iter()
            .map(|n| lower_ty(&n, ctx))
            .collect();
        if members.is_empty() {
            ctx.diag(HirDiagnostic::NotYetSupported {
                feature: "empty error-set union".to_string(),
                span: ctx.span_of(node),
            });
            HirTy::Error
        } else {
            HirTy::ErrorSetUnion(members)
        }
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
