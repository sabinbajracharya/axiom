//! Formatting helpers for THIR serialization: types, literals, indentation.

use axiom_hir::*;

pub(super) fn fmt_hir_ty(ty: &HirTy) -> String {
    match ty {
        HirTy::Named(nr) => match nr {
            NameRef::Resolved(r) => format!("{}→{}", r.text, r.def_id),
            NameRef::Unresolved(u) => format!("{}→<unresolved>", u.text),
        },
        HirTy::Unit => "()".to_string(),
        HirTy::Tuple(ts) => {
            format!(
                "({})",
                ts.iter().map(fmt_hir_ty).collect::<Vec<_>>().join(", ")
            )
        }
        HirTy::Fn(f) => {
            let params = f
                .params
                .iter()
                .map(fmt_hir_ty)
                .collect::<Vec<_>>()
                .join(", ");
            format!("fn({}) -> {}", params, fmt_hir_ty(&f.return_type))
        }
        HirTy::TypeParam(tp) => format!("{}→{}", tp.name, tp.id),
        HirTy::Instance(inst) => {
            let args = inst
                .args
                .iter()
                .map(fmt_hir_ty)
                .collect::<Vec<_>>()
                .join(", ");
            match &inst.name {
                NameRef::Resolved(r) => format!("{}→{}<{}>", r.text, r.def_id, args),
                NameRef::Unresolved(u) => format!("{}→<unresolved><{}>", u.text, args),
            }
        }
        HirTy::Error => "<error>".to_string(),
    }
}

pub(super) fn fmt_lit(kind: &LitKind) -> String {
    match kind {
        LitKind::Int(i) => format!("Int({i})"),
        LitKind::Float(f) => format!("Float({f})"),
        LitKind::Bool(b) => format!("Bool({b})"),
        LitKind::String(s) => format!("String(\"{}\")", s),
        LitKind::Unit => "Unit".to_string(),
    }
}

pub(super) fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}
