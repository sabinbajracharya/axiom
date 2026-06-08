//! Pattern serialization for the HIR snapshot dump. Extracted from `mod.rs`
//! to stay under the 600-line cap (RUST_CONVENTIONS.md §10).

use super::formatting::fmt_lit;
use crate::hir_types::*;

pub(super) fn serialize_pattern_inline(pat: &Pattern, out: &mut String) {
    match pat {
        Pattern::Wildcard(id) => out.push_str(&format!("Wild({id})")),
        Pattern::Ident(p) => {
            let binding = p.binding.map(|b| format!("→{b}")).unwrap_or_default();
            out.push_str(&format!("Ident({}) {}{}", p.id, p.name, binding));
        }
        Pattern::Literal(p) => out.push_str(&format!("Lit({}) {}", p.id, fmt_lit(&p.kind))),
        Pattern::TupleStruct(p) => {
            fmt_name_ref(&p.path, out);
            out.push('(');
            for (i, f) in p.fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                serialize_pattern_inline(f, out);
            }
            out.push(')');
        }
        Pattern::Struct(p) => {
            fmt_name_ref(&p.path, out);
            out.push_str(" { ");
            for (i, f) in p.fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&f.name);
                out.push_str(": ");
                serialize_pattern_inline(&f.pattern, out);
            }
            out.push_str(" }");
        }
        Pattern::Or(p) => {
            for (i, alt) in p.alternatives.iter().enumerate() {
                if i > 0 {
                    out.push_str(" | ");
                }
                serialize_pattern_inline(alt, out);
            }
        }
        Pattern::Range(p) => {
            if let Some(s) = &p.start {
                out.push_str(&fmt_lit(s));
            }
            out.push_str(if p.inclusive { "..=" } else { ".." });
            if let Some(e) = &p.end {
                out.push_str(&fmt_lit(e));
            }
        }
    }
}

fn fmt_name_ref(nr: &NameRef, out: &mut String) {
    match nr {
        NameRef::Resolved(r) => out.push_str(&format!("{}→{}", r.text, r.def_id)),
        NameRef::Unresolved(u) => out.push_str(&format!("{}→<unresolved>", u.text)),
    }
}
