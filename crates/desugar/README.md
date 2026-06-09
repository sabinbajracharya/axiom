# desugar

HIR desugaring: rewrites sugar expressions into core HIR nodes.

## Module layout

| Module | Purpose |
|--------|---------|
| `lib.rs` | Public API: re-exports `pre_typecheck` and `post_typecheck` |
| `helpers.rs` | Shared utilities: `DesugarCtx`, `replace_unresolved_name` |
| `pre_typecheck.rs` | Pre-typecheck desugaring: `catch`, `else`, `ListLit` |
| `post_typecheck.rs` | Post-typecheck desugaring: `?` expressions |

## How to add new sugar

1. Determine whether the sugar needs type information:
   - **No** → add to `pre_typecheck.rs` (runs before typecheck, needs `LangItems`)
   - **Yes** → add to `post_typecheck.rs` (runs after typecheck, needs `TypeMap`)
2. Add a new `Expr` variant in `lower/src/hir_types/mod.rs`
3. Handle the variant in both `pre_typecheck.rs` and `post_typecheck.rs` walk functions
4. Update the coverage invariant test in `tests_coverage.rs`
5. Add test fixtures and golden snapshots

## Pipeline position

```
driver::check_modules
  ├── parser::parse
  ├── resolver::lower_structural
  ├── resolver::build_global_exports
  ├── resolver::resolve_with_globals
  ├── validate_module_annotations
  ├── resolver::resolve_lang_items
  ├── desugar::pre_typecheck(&mut hir, &lang_items, next_id)    ← HERE
  ├── typecheck::check_with_lang_items(hir, lang_items)
  └── desugar::post_typecheck(&mut thir.hir, &thir.types, next_id)  ← HERE
```
