//! Helpers extracted from expr.rs.

use resolver::HirId;

/// Look up a FnDef's `module_path` by HirId across all HIR items.
/// Returns `None` if the FnDef is not found or has an empty module_path.
pub(super) fn find_fn_module_path(id: Option<HirId>, items: &[resolver::Item]) -> Option<String> {
    let id = id?;
    for item in items {
        match item {
            resolver::Item::FnDef(f) if f.id == id => {
                return Some(f.module_path.clone());
            }
            resolver::Item::ImplDef(impl_def) => {
                for m in &impl_def.methods {
                    if m.id == id {
                        return Some(m.module_path.clone());
                    }
                }
            }
            _ => {}
        }
    }
    None
}
