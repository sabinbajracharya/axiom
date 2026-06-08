# driver — Pipeline Orchestrator

**Purpose:** The single multi-module compilation driver. Orchestrates parse→lower→
resolve→annotation validation→desugar→type-check. All compilation paths (single-file,
project, stdlib-backed tests) funnel through `check_modules`.

Depends on all pipeline crates.

## Files

| File | Responsibility |
|------|---------------|
| `src/lib.rs` | `check_modules`, `check_source`, `validate_module_annotations`, `is_stdlib_module` |

## Key entry points

- `check_modules(&[(name, source)])` — multi-module pipeline, returns `Thir`
- `check_source(source)` — single-file bare (no-stdlib) mode

## Invariants

- All `@lang`/`@intrinsic` annotations validated before type checking
- Desugar runs after lang-item resolution, before `check_with_lang_items`
- `is_stdlib_module` uses build-time verified `phf::Set` — unspoofable
