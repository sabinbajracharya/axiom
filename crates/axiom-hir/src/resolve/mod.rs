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
use crate::lower::{Def, DefKind};
use crate::HirDiagnostic;
use axiom_lexer::Span;
use std::collections::{HashMap, HashSet};

/// Maps module name → { item name → (DefId, DefKind, Visibility) }.
/// Used for cross-module import resolution.
pub type GlobalExports = HashMap<String, HashMap<String, (DefId, DefKind, Visibility)>>;

/// Build a global export map from multiple modules' definitions.
/// Only includes `pub` items of kind Fn, Struct, Enum, Trait, or Variant.
pub fn build_global_exports(modules: &[(String, Vec<Def>)]) -> GlobalExports {
    let mut exports: GlobalExports = HashMap::new();
    for (module_name, defs) in modules {
        let module_exports = exports.entry(module_name.clone()).or_default();
        for def in defs {
            if !matches!(
                def.kind,
                DefKind::Fn | DefKind::Struct | DefKind::Enum | DefKind::Trait | DefKind::Variant
            ) {
                continue;
            }
            if def.visibility != Visibility::Public {
                continue;
            }
            module_exports.insert(def.name.clone(), (def.def_id, def.kind, def.visibility));
        }
    }
    exports
}

/// Run name resolution over the HIR built by lowering.
/// Mutates the HIR in-place: resolves `NameRef::Unresolved` entries
/// to `NameRef::Resolved` where names are found, and emits diagnostics
/// where they are not.
///
/// When `global_exports` is provided, pub items from the `"std::io"` module are
/// injected into scope at lowest priority — an implicit `use std::io::*` so that
/// single-file programs can call `println` without an explicit import.
pub fn resolve(ctx: &mut crate::lower::LowerCtx, global_exports: Option<&GlobalExports>) {
    // Pass 1: top-level item defs are already collected in ctx.defs during lowering.
    let mut top_level = build_top_level(&ctx.defs, &mut ctx.diagnostics);

    // Pass 1.25: inject the implicit prelude (`io` pub items) at lowest priority.
    inject_prelude(&mut top_level, global_exports);

    // Pass 1.5: process `use` items to add imported names to the top-level scope.
    process_use_items(
        &ctx.items,
        &mut top_level,
        &mut ctx.diagnostics,
        global_exports,
        "",
    );

    // Pass 2: resolve name references in all items.
    for item in &mut ctx.items {
        item::resolve_item_names(item, &top_level, &mut ctx.diagnostics);
    }
}

/// Run name resolution with cross-module context.
/// Takes pre-built items, defs, diagnostics, and a global export map.
pub fn resolve_with_globals(
    items: &mut [Item],
    defs: &[Def],
    diagnostics: &mut Vec<HirDiagnostic>,
    global_exports: &GlobalExports,
    current_module: &str,
) {
    // Pass 1: build top-level scope from this module's defs.
    let mut top_level = build_top_level(defs, diagnostics);

    // Pass 1.25: inject the implicit prelude (`io` pub items) — same rule as the
    // single-file `resolve` path, so `print` resolves identically everywhere.
    inject_prelude(&mut top_level, Some(global_exports));

    // Pass 1.5: process `use` items with cross-module lookup.
    process_use_items(
        items,
        &mut top_level,
        diagnostics,
        Some(global_exports),
        current_module,
    );

    // Pass 2: resolve name references in all items.
    for item in items.iter_mut() {
        item::resolve_item_names(item, &top_level, diagnostics);
    }

    // Pass 3: tag FnDefs with their module path for IR name qualification.
    for item in items.iter_mut() {
        if let Item::FnDef(f) = item {
            f.module_path = current_module.to_string();
        }
    }
}

/// The implicit prelude: modules whose pub items are in scope everywhere
/// without an explicit `use`. `core::traits` (Deinit/Equatable/Hashable/Ord —
/// the always-available behavioral vocabulary), `core::option` (`Option`/`Some`/
/// `None`), `core::result` (`Result`/`Ok`/`Err`), and `std::io` (print/println).
const PRELUDE_MODULES: &[&str] = &["core::traits", "core::option", "core::result", "std::io"];

/// Inject the implicit prelude into a module's top-level scope: the pub items of
/// each `PRELUDE_MODULES` entry, at lowest priority — `or_insert` so a module's
/// own definitions and explicit `use`s always win. Shared by both the
/// single-file (`resolve`) and multi-module (`resolve_with_globals`) paths so
/// the prelude names resolve identically everywhere.
/// See `docs/stdlib-loading-unification.md`.
fn inject_prelude(
    top_level: &mut HashMap<String, (DefId, DefKind)>,
    global_exports: Option<&GlobalExports>,
) {
    let Some(exports) = global_exports else {
        return;
    };
    for module in PRELUDE_MODULES {
        let Some(items) = exports.get(*module) else {
            continue;
        };
        for (name, &(def_id, kind, _vis)) in items {
            top_level.entry(name.clone()).or_insert((def_id, kind));
        }
    }
}

/// Build the top-level scope from a module's definitions.
fn build_top_level(
    defs: &[Def],
    diagnostics: &mut Vec<HirDiagnostic>,
) -> HashMap<String, (DefId, DefKind)> {
    let mut top_level: HashMap<String, (DefId, DefKind)> = HashMap::new();
    for def in defs {
        if matches!(
            def.kind,
            DefKind::Fn | DefKind::Struct | DefKind::Enum | DefKind::Trait | DefKind::Variant
        ) {
            if let Some((prev_def_id, _)) = top_level.get(&def.name) {
                let prev_span = defs
                    .iter()
                    .find(|d| &d.def_id == prev_def_id)
                    .map(|d| d.span)
                    .unwrap_or(def.span);
                diagnostics.push(HirDiagnostic::DuplicateDefinition {
                    name: def.name.clone(),
                    span: def.span,
                });
                diagnostics.push(HirDiagnostic::DuplicateDefinition {
                    name: format!("{} (previous definition here)", def.name),
                    span: prev_span,
                });
            } else {
                top_level.insert(def.name.clone(), (def.def_id, def.kind));
            }
        }
    }
    top_level
}

// ── Use item processing ──────────────────────────────────────────────────────

/// Process `use` items: resolve import paths and add imported names to the scope.
fn process_use_items(
    items: &[Item],
    top_level: &mut HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
    global_exports: Option<&GlobalExports>,
    current_module: &str,
) {
    for item in items {
        if let Item::UseItem(u) = item {
            process_use_tree(
                &u.tree,
                top_level,
                diagnostics,
                global_exports,
                current_module,
            );
        }
    }
}

/// Process a single use tree, adding imported names to the scope.
fn process_use_tree(
    tree: &UseTree,
    top_level: &mut HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
    global_exports: Option<&GlobalExports>,
    current_module: &str,
) {
    match &tree.kind {
        UseTreeKind::Single { rename } => {
            if let Some((def_id, kind)) = resolve_use_path(
                &tree.path,
                top_level,
                global_exports,
                current_module,
                diagnostics,
            ) {
                let name = rename
                    .clone()
                    .unwrap_or_else(|| tree.path.last().cloned().unwrap_or_default());
                top_level.insert(name, (def_id, kind));
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
                process_use_tree(
                    &combined,
                    top_level,
                    diagnostics,
                    global_exports,
                    current_module,
                );
            }
        }
        UseTreeKind::Glob => {
            // Glob imports require module graph support — deferred.
            diagnostics.push(HirDiagnostic::NotYetSupported {
                feature: "glob imports (`use foo::*`)".to_string(),
                span: Span { lo: 0, hi: 0 },
            });
        }
    }
}

/// Resolve a use path to a definition.
///
/// - Single-segment paths look up directly in `top_level` (intra-module).
/// - Multi-segment paths resolve the first segment(s) as a module name in
///   `global_exports`, then look up the final segment in that module's exports.
fn resolve_use_path(
    path: &[String],
    top_level: &HashMap<String, (DefId, DefKind)>,
    global_exports: Option<&GlobalExports>,
    current_module: &str,
    diagnostics: &mut Vec<HirDiagnostic>,
) -> Option<(DefId, DefKind)> {
    if path.is_empty() {
        return None;
    }

    // Single segment: look up in local top_level (same as before).
    if path.len() == 1 {
        let name = &path[0];
        return top_level.get(name).copied();
    }

    // Multi-segment: resolve module path + item name.
    let exports = global_exports?;

    // The last segment is the item name; everything before is the module path.
    let item_name = &path[path.len() - 1];
    let module_path = &path[..path.len() - 1];

    // Try to find the module by joining segments.
    let module_key = module_path.join("::");

    if let Some(module_items) = exports.get(&module_key) {
        if let Some(&(def_id, kind, vis)) = module_items.get(item_name) {
            // Visibility check: private items from other modules are not accessible.
            if vis == Visibility::Private && module_key != current_module {
                diagnostics.push(HirDiagnostic::PrivateImport {
                    name: item_name.clone(),
                    module: module_key,
                    span: Span { lo: 0, hi: 0 },
                });
                return None;
            }
            return Some((def_id, kind));
        }
    }

    // Not found — emit unresolved.
    diagnostics.push(HirDiagnostic::UnresolvedName {
        name: path.join("::"),
        span: Span { lo: 0, hi: 0 },
    });
    None
}

// ── Name resolution helpers ──────────────────────────────────────────────────

/// Try to resolve a NameRef against the scope's bindings or the builtins,
/// rewriting it to `Resolved` on success. Returns whether it resolved. Emits
/// no diagnostic — callers that require resolution use [`resolve_name_ref`].
pub(crate) fn try_resolve_name_ref(
    nr: &mut NameRef,
    bindings: &HashMap<String, (DefId, DefKind)>,
) -> bool {
    let text = match nr {
        NameRef::Resolved(_) => return true,
        NameRef::Unresolved(u) => u.text.clone(),
    };

    if let Some((def_id, _kind)) = bindings.get(&text) {
        *nr = NameRef::Resolved(ResolvedName {
            def_id: *def_id,
            text,
        });
        return true;
    }

    if let Some(def_id) = builtin_def_id(&text) {
        *nr = NameRef::Resolved(ResolvedName { def_id, text });
        return true;
    }

    false
}

/// Resolve a NameRef by looking it up in the given scope, emitting an
/// `UnresolvedName` diagnostic if it is not found.
pub(crate) fn resolve_name_ref(
    nr: &mut NameRef,
    bindings: &HashMap<String, (DefId, DefKind)>,
    diagnostics: &mut Vec<HirDiagnostic>,
) {
    if try_resolve_name_ref(nr, bindings) {
        return;
    }
    if let NameRef::Unresolved(u) = nr {
        diagnostics.push(HirDiagnostic::UnresolvedName {
            name: u.text.clone(),
            span: Span { lo: 0, hi: 0 },
        });
    }
}

/// Reserved HirId range for builtins. Real definitions start above this.
const BUILTIN_HIR_ID_START: usize = 1_000_000;

/// Built-in names that are always available (no module definition needed).
/// Primitive types + `todo` (compiler-internal stub) + `format` (the one
/// variadic formatting intrinsic — see `docs/string-format-and-print-retire.md`).
/// `print`/`println` resolve through `stdlib/std/io.ax` via the module system.
///
/// `format` is given a name here so a bare `format(...)` call (which is what
/// `string::format(...)` lowers to — the call lowerer keeps only the last path
/// segment) resolves to a definition and reaches the type checker, where it is
/// special-cased as variadic → `String`, rather than erroring as unresolved.
///
/// `heap_alloc`/`heap_get`/`heap_set`/`heap_free` are the `HeapBuffer<T>` floor
/// ops (P4) the collection library is built on — compiler intrinsics with no
/// module definition. They are named here so calls in `stdlib/std/collections`
/// resolve; the type checker gives them generic signatures (`helpers::builtin_fn`)
/// and IR lowering emits the dedicated heap instructions.
pub(crate) fn builtin_def_id(name: &str) -> Option<DefId> {
    let idx = match name {
        "Int" => 0,
        "Float" => 1,
        "Bool" => 2,
        "String" => 3,
        "Unit" => 4,
        "todo" => 5,
        "format" => 6,
        "heap_alloc" => 7,
        "heap_get" => 8,
        "heap_set" => 9,
        "heap_free" => 10,
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

    /// Define a name in this scope. Returns `true` if it shadows a name
    /// already defined in the same scope (an error).
    pub fn define(&mut self, name: String, def_id: DefId, kind: DefKind) -> bool {
        let shadows_same_scope = self.own_names.contains(&name);
        self.own_names.insert(name.clone());
        self.bindings.insert(name, (def_id, kind));
        shadows_same_scope
    }
}
