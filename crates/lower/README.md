# lower — Structural HIR Lowering

**Purpose:** Lowers the CST/AST from `parser` into an HIR tree where every node has a
stable `HirId` but names are still `Unresolved`. Name resolution lives in `resolver`.

## Files

| File | Responsibility |
|------|---------------|
| `src/lib.rs` | Public API: `lower_structural`, `Hir`, `HirDiagnostic`, `check_all`, `serialize` |
| `src/error.rs` | `HirDiagnostic` enum — lowering + resolution diagnostics |
| `src/hir_types/` | Core HIR types: `Hir`, `Item`, `FnDef`, `Expr`, `Block`, `Stmt`, `NameRef`, `HirId`, etc. |
| `src/lowering/` | Lowering pass: CST→HIR transformation (`lower_structural`, `LowerCtx`) |
| `src/lowering/error.rs` | Error set definition lowering (`lower_error_set_def`) |
| `src/serialize/` | Canonical HIR dump format (used as golden-test oracle) |
| `src/serialize/patterns.rs` | Pattern serialization for the HIR dump |

## Key entry points

- `lower_structural(root, source, start_id)` — structural lowering only, returns `(items, defs, diags, next_id)`
- `serialize(&Hir)` — canonical HIR snapshot dump
- `check_all(&Hir)` — coverage invariant: every `Unresolved` has a diagnostic

## Invariants

- Every node has a unique `HirId` within the lowering scope
- Every `NameRef::Unresolved` has a corresponding `HirDiagnostic::UnresolvedName`
- Serialize output is deterministic and LF-only
