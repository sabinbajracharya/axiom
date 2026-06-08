# resolver — Name Resolution + Lang Items + Intrinsics + Desugar

**Purpose:** Resolves every `NameRef::Unresolved` in a lowered HIR to a
`NameRef::Resolved(DefId, text)` by walking the scope chain. Also houses
`@lang`/`@intrinsic` collection and validation, cross-module exports, and the
desugar pass.

Depends on `lower` for base HIR types.

## Files

| File | Responsibility |
|------|---------------|
| `src/lib.rs` | Public API + `lower()` (combined lowering+resolution) |
| `src/resolve/` | Name resolution: scope chain, use processing, global exports |
| `src/lang.rs` | `@lang` item collection, validation, and registry |
| `src/intrinsic.rs` | `@intrinsic` key registry, collection, validation, drift guard |
| `src/desugar/` | Desugar pass: `ListLit`→`List::new+push`, runs after resolve before typeck |

## Key entry points

- `lower(root, source, global_exports)` — full lower+resolve in one call
- `resolve_with_globals(items, defs, diagnostics, exports, module_name)` — multi-file resolution
- `build_global_exports(modules)` — cross-module export map
- `collect_lang_bindings(items)` / `resolve_lang_items(bindings, enforce)`
- `collect_intrinsic_bindings(items)` / `validate_intrinsic_bindings(bindings)`
- `desugar(&mut Hir, &LangItems, next_id)` — sugar rewrites

## Invariants

- Every `NameRef::Unresolved` is resolved to `Resolved` or diagnosed
- `@lang`/`@intrinsic` only valid in stdlib modules (enforced in driver)
- Desugar runs after resolution, before type checking
