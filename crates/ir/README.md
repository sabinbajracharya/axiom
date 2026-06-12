# ir — Register IR (THIR → CFG)

The register-based intermediate representation and its lowerer. Consumes the
**THIR** (typed HIR, from `typecheck`) plus the monomorphization result (from
`specialize`) and produces a control-flow-graph IR: functions made of basic
blocks, each block a list of `IrInstr`s ending in a `Terminator`, over infinite
virtual registers.

This is the **single lowering target both backends consume** and the layer v1's
ownership + Perceus passes will run on. See [`docs/ir-design.md`](../../docs/ir-design.md)
for the full design and [`docs/foundation-hardening.md`](../../docs/foundation-hardening.md)
(F5.2) for the planned invariant hardening.

## Scope (v0)

- CFG IR: `Ir` → `IrFunction` → `IrBlock { instrs, terminator }`, fresh `Reg`
  per definition.
- Monomorphized generic instances appear as **separate concrete IR functions**
  (carrying their `GenericOrigin`).
- Lowering of the typed subset: bindings, arithmetic/compare/logic, `if`/`loop`
  → branch/jump CFG, `match` → decision-tree branches, calls, struct/enum
  build + field/variant access, pattern destructure.
- **Naive memory:** aggregates are values with copy semantics; no elision, no
  reuse, no exclusivity (those are v1, and hook in here).
- Structural well-formedness invariants only (see `invariants.rs`).

## Files

| File | Responsibility | Key items |
|---|---|---|
| `src/lib.rs` | Crate root; public API re-exports | `lower`, `serialize`, `check_invariants` |
| `src/ir.rs` | The IR data model | `Ir`, `IrFunction`, `IrBlock`, `IrInstr`, `IrConst`, `Terminator`, `Reg`, `MatchArm`, `IrPattern`, `GenericOrigin` |
| `src/invariants.rs` | Structural well-formedness checks | `check_invariants` (terminator targets, register-defined-before-use, call targets) |
| `src/lower/mod.rs` | Lowering entry point + lowerer state | `lower(thir, mono) -> Ir` |
| `src/lower/item.rs` | Lower items (fns, impls) → `IrFunction`s | — |
| `src/lower/stmt.rs` | Lower statements | — |
| `src/lower/expr.rs` | Lower expressions (the bulk: calls, `match`, control flow) | — |
| `src/lower/expr_helpers.rs` | Expression-lowering helpers | — |
| `src/lower/assign.rs` | Lower assignments / place expressions | — |
| `src/lower/helpers.rs` | Shared lowering helpers (blocks, regs, consts) | — |
| `src/serialize/mod.rs` | Canonical CFG-readable IR dump (the debug + golden oracle) | `serialize` |
| `src/serialize/helpers.rs` | Serialization helpers | — |

## Testing

- Golden IR snapshots over the corpus (`tests/golden.rs`, `tests/fixtures/`).
- Desugar/lowering coverage drift guard (`tests/desugar_coverage.rs`,
  `tests/desugar_goldens/`).
- IR well-formedness invariant checks (`tests/invariants.rs`).
- `examples/ir.rs` debug CLI: `cargo run -p ir --example ir -- file.ax` dumps
  the lowered CFG for any `.ax` file.

## Run tests

```bash
cargo test -p ir
```
