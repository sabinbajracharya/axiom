# axiom-typeck

The Axiom type checker (M2): walks the HIR, assigns a type to every expression
and statement, and collects type diagnostics. Built test-first against
[`docs/typeck-testing.md`](../../docs/typeck-testing.md).

Three properties define it:
- **Every expression gets a type.** The `TypeMap` (a `HashMap<HirId, Ty>`) assigns
  a type to every HIR node. If type checking fails for an expression, it gets
  `Ty::Error` paired with a `TypeDiagnostic` — never silently skipped.
- **Bidirectional typing.** `infer(expr)` computes the type from subexpressions;
  `check(expr, expected)` verifies against an expected type. Explicit fn signatures
  required; local inference for `val`/`var` initializers.
- **Total.** Every input produces a `Thir` (HIR + TypeMap + diagnostics), never a
  panic. Type errors are in `thir.diagnostics`, not crash results.

## How it works (end-to-end flow)

```
HIR (from axiom-hir) → axiom_typeck::check → Thir { hir, types, diagnostics }
Thir → axiom_typeck::serialize → canonical THIR dump (String)
Thir → axiom_typeck::check_all → coverage invariant check
```

`check` is a two-pass pipeline (mirroring the HIR's own collect→resolve pattern):
1. **Collect pass:** Register fn signatures, struct definitions, and enum
   definitions in the type environment. This allows forward references.
2. **Check pass:** Walk fn bodies, type-checking each expression against the
   environment. Type errors emit `TypeDiagnostic`s and assign `Ty::Error`.

## Files

| File | Responsibility | Key items |
|---|---|---|
| `src/lib.rs` | Crate root; public API (`check`, `serialize`, `check_all`) | `check`, `serialize`, `check_all` |
| `src/types.rs` | The type universe: `Ty`, `StructTy`, `EnumTy`, `FnTy`, Display impls | `Ty`, `label()` |
| `src/error.rs` | Type-check diagnostics (`thiserror` enum) + render | `TypeDiagnostic` |
| `src/thir.rs` | THIR wrapper (HIR + TypeMap + diagnostics) | `Thir`, `TypeMap` |
| `src/typeck/` | The type checker module folder | — |
| `src/typeck/mod.rs` | Entry point: `TypeChecker` struct, `TypeEnv`, two passes, inline tests | `check()`, bidirectional typing |
| `src/typeck/collect.rs` | Pass 1: collect fn signatures, struct/enum defs | `collect_pass` |
| `src/typeck/infer.rs` | Expression type rules: literals, paths, binary/unary ops, calls, fields | `infer_expr`, `check_expr` |
| `src/typeck/control.rs` | Control-flow type rules: blocks, if/else, match, loop, struct lit, assign | `infer_block`, `infer_if`, `infer_match` |
| `src/typeck/stmt.rs` | Statement typing and pattern binding | `type_stmt`, `define_pattern_bindings` |
| `src/typeck/helpers.rs` | Small pure helpers: `is_error`, `is_numeric`, `infer_lit`, `call_name` | — |
| `src/exhaustiveness.rs` | Match exhaustiveness checking for enums | `check_match_exhaustiveness` |
| `src/serialize.rs` | Canonical THIR dump (pure function) | `serialize` |
| `src/coverage.rs` | Coverage invariant checks | `check_all`, `TypeckCoverageError` |
| `examples/typeck.rs` | Debug CLI (`cargo run -p axiom-typeck --example typeck -- file.ax`) | — |

## Commands

```bash
cargo test -p axiom-typeck                            # full suite
cargo test -p axiom-typeck --test fuzz                # fuzz/property tests only
UPDATE_SNAPSHOTS=1 cargo test -p axiom-typeck          # regenerate .thir / .stderr
cargo run -p axiom-typeck --example typeck -- file.ax  # debug THIR dump
cargo run -p axiom-cli -- check file.ax                # CST + HIR + THIR dumps + diagnostics
cargo clippy -p axiom-typeck -- -D warnings           # lint
```

## When you change this crate

- Add a `Ty` variant: add it to `Ty`, add a `label()` arm, update `Display`,
  add golden fixtures that exercise it. The drift guard will pass when the
  checker handles the new type.
- Add a new HIR expression kind (upstream): the drift guard
  `test_typecker_handles_every_hir_expr_kind` will fail until the type checker
  has a typing rule for it.
- Add a new type diagnostic kind: add a variant to `TypeDiagnostic`, add a
  fixture in `errors/*.ax` + checked-in `.stderr`, regenerate with
  `UPDATE_SNAPSHOTS=1`.