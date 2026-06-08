# specialize — Monomorphization (Generic Specialization)

**Purpose:** Discovers every concrete instantiation of generic functions and produces
`MonoInstance` records. The IR lowering pass uses these to generate one specialized
function copy per unique `(fn_id, concrete_type_args)` pair.

Depends on `typecheck` (reads `&Thir`, produces `MonoResult`) and `resolver` (HIR types).
The result types (`MonoResult`, `MonoInstance`) are defined in `typecheck::mono_types`
to avoid a circular dependency.

## Files

| File | Responsibility |
|------|---------------|
| `src/lib.rs` | Public API: `monomorphize`, re-exports `MonoInstance`/`MonoResult` |
| `src/mono.rs` | Main monomorphization algorithm: `Monomorphizer` struct + `monomorphize()` |
| `src/helpers.rs` | `Substitution` type, `unify`, `substitute`, `mangle_name` |
| `src/walk.rs` | Expression/statement tree walkers for call-site discovery |
| `src/tests.rs` | Unit tests for helpers |

## Key entry points

- `monomorphize(&Thir) -> MonoResult` — discover all generic instantiations

## Invariants

- Each unique `(generic_fn_id, concrete_type_args)` pair produces exactly one instance
- Deduplication: multiple calls with same types → single instance
- Nested generic calls are discovered transitively
- Non-generic programs yield empty `MonoResult`
