//! Formatting helpers extracted from `mod.rs`.

use super::escape;
use crate::hir_types::LitKind;

pub(super) fn fmt_lit(kind: &LitKind) -> String {
    match kind {
        LitKind::Int(i) => format!("Int({i})"),
        LitKind::Float(f) => format!("Float({f})"),
        LitKind::Bool(b) => format!("Bool({b})"),
        LitKind::String(s) => format!("String(\"{}\")", escape::escape_str(s)),
        LitKind::Unit => "Unit".to_string(),
    }
}

pub(super) fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}
