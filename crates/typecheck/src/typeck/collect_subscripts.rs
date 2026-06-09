//! Subscript validation helpers extracted from collect.rs.

use super::{TypeChecker, TypeDiagnostic};
use resolver::*;

use std::collections::HashMap;

/// Check for duplicate subscripts with the same index-param count in an impl.
/// Two read subscripts (or two write subscripts) with the same index-param count
/// produce diagnostics. Never silently pick one and ignore the other.
pub(super) fn check_duplicate_subscripts(
    subscripts: &[SubscriptDef],
    type_name: &str,
    tc: &mut TypeChecker,
) {
    let mut read_counts: HashMap<usize, HirId> = HashMap::new();
    let mut write_counts: HashMap<usize, HirId> = HashMap::new();
    for s in subscripts {
        let index_count = s.params.len().saturating_sub(1);
        if s.is_setter {
            if let Some(&prev) = write_counts.get(&index_count) {
                tc.emit(TypeDiagnostic::DuplicateSubscript {
                    type_name: type_name.to_string(),
                    index_param_count: index_count,
                    kind: "write".to_string(),
                    span: tc.span_for(s.id),
                });
                tc.emit(TypeDiagnostic::DuplicateSubscript {
                    type_name: type_name.to_string(),
                    index_param_count: index_count,
                    kind: "write".to_string(),
                    span: tc.span_for(prev),
                });
            } else {
                write_counts.insert(index_count, s.id);
            }
        } else if let Some(&prev) = read_counts.get(&index_count) {
            tc.emit(TypeDiagnostic::DuplicateSubscript {
                type_name: type_name.to_string(),
                index_param_count: index_count,
                kind: "read".to_string(),
                span: tc.span_for(s.id),
            });
            tc.emit(TypeDiagnostic::DuplicateSubscript {
                type_name: type_name.to_string(),
                index_param_count: index_count,
                kind: "read".to_string(),
                span: tc.span_for(prev),
            });
        } else {
            read_counts.insert(index_count, s.id);
        }
    }
}

/// Extract the text from a `NameRef` (resolved or unresolved).
pub(super) fn name_text(nr: &NameRef) -> String {
    match nr {
        NameRef::Resolved(r) => r.text.clone(),
        NameRef::Unresolved(u) => u.text.clone(),
    }
}

/// Extract the DefId from a resolved `NameRef`, or `None` if unresolved.
pub(super) fn name_def_id(nr: &NameRef) -> Option<DefId> {
    match nr {
        NameRef::Resolved(r) => Some(r.def_id),
        NameRef::Unresolved(_) => None,
    }
}

/// Whether `name` is a builtin primitive type.
pub(super) fn is_builtin_type_name(name: &str) -> bool {
    matches!(name, "Int" | "Float" | "Bool" | "String" | "Unit")
}
