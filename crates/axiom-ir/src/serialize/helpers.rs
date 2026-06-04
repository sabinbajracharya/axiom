//! Formatting helpers for the IR serializer.

use crate::ir::Reg;
use axiom_typeck::Ty;

/// Format a register as `%N`.
pub fn fmt_reg(r: Reg) -> String {
    format!("%{}", r.0)
}

/// Indent: two spaces per depth level.
pub fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

/// Format a type for display.
pub fn fmt_ty(ty: &Ty) -> String {
    match ty {
        Ty::Int => "Int".to_string(),
        Ty::Float => "Float".to_string(),
        Ty::Bool => "Bool".to_string(),
        Ty::String => "String".to_string(),
        Ty::Unit => "Unit".to_string(),
        Ty::Struct(s) => s.name.clone(),
        Ty::Enum(e) => e.name.clone(),
        Ty::Fn(f) => {
            let params: Vec<String> = f.params.iter().map(fmt_ty).collect();
            format!("fn({}) -> {}", params.join(", "), fmt_ty(&f.return_type))
        }
        Ty::Tuple(tys) => {
            let parts: Vec<String> = tys.iter().map(fmt_ty).collect();
            format!("({})", parts.join(", "))
        }
        Ty::TypeParam(tp) => format!("<{}>", tp.name),
        Ty::Instance(i) => {
            let args: Vec<String> = i.args.iter().map(fmt_ty).collect();
            format!("{}<{}>", i.name, args.join(", "))
        }
        Ty::Error => "<error>".to_string(),
    }
}
