//! Name resolution: two-pass resolution of identifiers to definitions.
//!
//! Pass 1 collects top-level item definitions (fn, struct, enum) into a symbol table.
//! Pass 1.5 processes `use` items to add imported names.
//! Pass 2 resolves name references in bodies against lexical scopes.
//!
//! Per `docs/hir-testing.md` §4: same-scope shadowing is disallowed;
//! nested-scope shadowing is allowed.

mod body;
mod item;

use crate::hir::*;
use crate::lower::DefKind;
use crate::HirDiagnostic;
use axiom_lexer::Span;
use std::collections::{HashMap, HashSet};

/// Run name resolution over the HIR built by lowering.
/// Mutates the HIR in-place: resolves `NameRef::Unresolved` entries
/// to `NameRef::Resolved` where names are found, and emits diagnostics
/// where they are not.
pub fn resolve(ctx: &mut crate::lower::LowerCtx) {
    // Pass 1: top-level item defs are already collected in ctx.defs during lowering.
    // Build a top-level scope map.
    let mut top_level: HashMap<String, (DefId, DefKind)> = HashMap::new();
    for def in &ctx.defs {
        if matches!(
            def.kind,
            DefKind::Fn | DefKind::Struct | DefKind::Enum | DefKind::Trait | DefKind::Variant
        ) {
            if let Some((prev_def_id, _)) = top_level.get(&def.name) {
                let prev_def = ctx.defs.iter().find(|d| &d.def_id == prev_def_id);
                let prev_span = prev_def.map(|d| d.span).unwrap_or(def.span);
                ctx.diagnostics.push(HirDiagnostic::DuplicateDefinition {
                    name: def.name.clone(),
                    span: def.span,
                });
                ctx.diagnostics.push(HirDiagnostic::DuplicateDefinition {
                    name: format!("{} (previous definition here)", def.name),
                    span: prev_span,
                });
            } else {
                top_level.insert(def.name.clone(), (def.def_id, def.kind));
            }
        }
    }

    // Pass 1.5: process `use` items to add imported names to the top-level scope.
    process_use_items(&ctx.items, &mut top_level, &mut ctx.diagnostics);

    // Pass 2: resolve name references in all items.
    for item in &mut ctx.items {
        item::resolve_item_names(item, &top_level, &mut ctx.diagnostics);
    }
}

// ── Use item processing ──────────────────────────────────────────────────────

/// Process `use` items: resolve import paths and add imported names to the scope.
fn process_use_items(
    items: &[Item],
    top_level: &mut HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    for item in items {
        if let Item::UseItem(u) = item {
            process_use_tree(&u.tree, top_level, diagnostics);
        }
    }
}

/// Process a single use tree, adding imported names to the scope.
fn process_use_tree(
    tree: &UseTree,
    top_level: &mut HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    match &tree.kind {
        UseTreeKind::Single { rename } => {
            if let Some((def_id, kind)) = resolve_use_path(&tree.path, top_level) {
                let name = rename
                    .clone()
                    .unwrap_or_else(|| tree.path.last().cloned().unwrap_or_default());
                top_level.insert(name, (def_id, kind));
            } else {
                diagnostics.push(HirDiagnostic::UnresolvedName {
                    name: tree.path.join("::"),
                    span: Span { lo: 0, hi: 0 },
                });
            }
        }
        UseTreeKind::Group(trees) => {
            for sub_tree in trees {
                let mut full_path = tree.path.clone();
                full_path.extend(sub_tree.path.iter().cloned());
                let combined = UseTree {
                    path: full_path,
                    kind: sub_tree.kind.clone(),
                };
                process_use_tree(&combined, top_level, diagnostics);
            }
        }
        UseTreeKind::Glob => {
            // Glob imports require module graph support — deferred.
        }
    }
}

/// Resolve a use path to a definition.
/// Single-segment paths look up directly in top_level.
/// Multi-segment paths resolve the last segment (simplified until module graph).
fn resolve_use_path(
    path: &[String],
    top_level: &HashMap<String, (DefId, DefKind)>,
) -> Option<(DefId, DefKind)> {
    if path.is_empty() {
        return None;
    }
    let name = path.last()?;
    top_level.get(name).copied()
}

// ── Name resolution helpers ──────────────────────────────────────────────────

/// Resolve a NameRef by looking it up in the given scope.
pub(crate) fn resolve_name_ref(
    nr: &mut NameRef,
    bindings: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    let text = match nr {
        NameRef::Resolved(_) => return,
        NameRef::Unresolved(u) => u.text.clone(),
    };

    if let Some((def_id, _kind)) = bindings.get(&text) {
        *nr = NameRef::Resolved(ResolvedName {
            def_id: *def_id,
            text,
        });
        return;
    }

    if let Some(def_id) = builtin_def_id(&text) {
        *nr = NameRef::Resolved(ResolvedName { def_id, text });
        return;
    }

    diagnostics.push(HirDiagnostic::UnresolvedName {
        name: text,
        span: Span { lo: 0, hi: 0 },
    });
}

/// Reserved HirId range for builtins. Real definitions start above this.
const BUILTIN_HIR_ID_START: usize = 1_000_000;

/// Built-in names that are always available, mapped to reserved HirIds.
pub(crate) fn builtin_def_id(name: &str) -> Option<DefId> {
    let idx = match name {
        "print" => 0,
        "println" => 1,
        "Int" => 2,
        "Float" => 3,
        "Bool" => 4,
        "String" => 5,
        "Unit" => 6,
        _ => return None,
    };
    Some(HirId(BUILTIN_HIR_ID_START + idx))
}

// ── Scope ────────────────────────────────────────────────────────────────────

pub(crate) struct Scope {
    /// All bindings visible in this scope (own + inherited from parent).
    pub bindings: HashMap<String, (DefId, DefKind)>,
    /// Names defined in THIS scope only (not inherited).
    own_names: HashSet<String>,
}

impl Scope {
    pub fn new_child(parent: &HashMap<String, (DefId, DefKind)>) -> Self {
        Scope {
            bindings: parent.clone(),
            own_names: HashSet::new(),
        }
    }

    /// Define a binding in this scope. Returns `true` if this is a same-scope
    /// redefinition (error), `false` if it's a new name or shadowing a parent name.
    pub fn define(&mut self, name: String, id: DefId, kind: DefKind) -> bool {
        let redefines_own = self.own_names.contains(&name);
        self.bindings.insert(name.clone(), (id, kind));
        self.own_names.insert(name);
        redefines_own
    }
}
