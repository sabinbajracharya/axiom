//! Type formatting for the HIR snapshot serializer. Mirrors `HirTy` shape into
//! the golden-test dump (`docs/hir-testing.md` §2). Kept separate from `mod.rs`
//! so the serializer stays under the file-size cap (RUST_CONVENTIONS.md §10).

use crate::hir_types::*;

pub(super) fn fmt_ty(ty: &HirTy) -> String {
    match ty {
        HirTy::Named(nr) => match nr {
            NameRef::Resolved(r) => format!("{}→{}", r.text, r.def_id),
            NameRef::Unresolved(u) => format!("{}→<unresolved>", u.text),
        },
        HirTy::Unit => "()".to_string(),
        HirTy::Tuple(ts) => {
            format!("({})", ts.iter().map(fmt_ty).collect::<Vec<_>>().join(", "))
        }
        HirTy::Fn(f) => {
            let params = f.params.iter().map(fmt_ty).collect::<Vec<_>>().join(", ");
            format!("fn({}) -> {}", params, fmt_ty(&f.return_type))
        }
        HirTy::TypeParam(tp) => format!("{}→{}", tp.name, tp.id),
        HirTy::Instance(inst) => {
            let args = inst.args.iter().map(fmt_ty).collect::<Vec<_>>().join(", ");
            match &inst.name {
                NameRef::Resolved(r) => format!("{}→{}<{}>", r.text, r.def_id, args),
                NameRef::Unresolved(u) => format!("{}→<unresolved><{}>", u.text, args),
            }
        }
        HirTy::Slice(elem) => format!("[{}]", fmt_ty(elem)),
        HirTy::ErrorSet(nr) => match nr {
            NameRef::Resolved(r) => format!("{}→{}", r.text, r.def_id),
            NameRef::Unresolved(u) => format!("{}→<unresolved>", u.text),
        },
        HirTy::ErrorSetUnion(members) => {
            let inner = members.iter().map(fmt_ty).collect::<Vec<_>>().join(" || ");
            format!("({})", inner)
        }
        HirTy::Error => "<error>".to_string(),
    }
}

pub(super) fn fmt_ty_maybe(ty: &Option<HirTy>) -> String {
    match ty {
        Some(t) => fmt_ty(t),
        None => "_".to_string(),
    }
}
