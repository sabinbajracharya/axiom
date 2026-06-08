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
HIR (from resolver) → typecheck::check → Thir { hir, types, diagnostics }
Thir → typecheck::serialize → canonical THIR dump (String)
Thir → typecheck::check_all → coverage invariant check
```

`check` is a two-pass pipeline (mirroring the HIR's own collect→resolve pattern):
1. **Collect pass:** Register fn signatures, struct definitions, and enum
   definitions in the type environment. This allows forward references.
2. **Check pass:** Walk fn bodies, type-checking each expression against the
   environment. Type errors emit `TypeDiagnostic`s and assign `Ty::Error`.

Multi-module orchestration (parse → lower → resolve → validate → type-check) lives in the
`driver` crate. This crate is a pure type-checking pass: it consumes a resolved `Hir`,
walks every expression and statement, and assigns a `Ty` to every node via the `TypeMap`
side table. Type errors emit `TypeDiagnostic`s and assign `Ty::Error` — the tree is always
complete, never silently skipped.

**Standard library:** The stdlib is embedded by the `stdlib` crate; callers build the
module list with `stdlib::with_main(src)` and pass it to `driver::check_modules`.
Bare `typecheck::check_source(src)` is the deliberate no-stdlib mode for compiler-isolation
tests. See `docs/stdlib-loading-unification.md`.

## Files

| File | Responsibility | Key items |
|---|---|---|
| `src/lib.rs` | Crate root; public API (`check`, `check_with_lang_items`, `check_source`, `serialize`, `check_all`, `hir_max_id`) | `check`, `check_source`, `hir_max_id` |
| `src/types.rs` | The type universe: `Ty`, `StructTy`, `EnumTy`, `FnTy`, Display impls | `Ty`, `label()` |
| `src/error.rs` | Type-check diagnostics (`thiserror` enum) + render | `TypeDiagnostic` |
| `src/thir.rs` | THIR wrapper (HIR + TypeMap + diagnostics) | `Thir`, `TypeMap` |
| `src/typeck/` | The type checker module folder | — |
| `src/typeck/mod.rs` | Entry point: `TypeChecker` struct, `TypeEnv`, two passes, inline tests | `check()`, bidirectional typing |
| `src/typeck/collect.rs` | Pass 1: collect fn signatures, struct/enum defs | `collect_pass` |
| `src/typeck/infer.rs` | Expression type rules: literals, paths, binary/unary ops, calls, fields | `infer_expr`, `check_expr` |
| `src/typeck/control.rs` | Control-flow type rules: blocks, if/else, match, loop, struct lit, assign | `infer_block`, `infer_if`, `infer_match` |
| `src/typeck/typeinfo.rs` | Generic type-def introspection: a struct's fields / an enum's variant payloads resolved in the type's own type-param scope | `struct_generic_info`, `enum_generic_info` |
| `src/typeck/stmt.rs` | Statement typing and pattern binding | `type_stmt`, `define_pattern_bindings` |
| `src/typeck/helpers.rs` | Small pure helpers: `is_error`, `is_numeric`, `infer_lit`, `call_name` | — |
| `src/exhaustiveness.rs` | Match exhaustiveness checking for enums | `check_match_exhaustiveness` |
| `src/serialize.rs` | Canonical THIR dump (pure function) | `serialize` |
| `src/coverage.rs` | Coverage invariant checks | `check_all`, `TypeckCoverageError` |
| `examples/typeck.rs` | Debug CLI (`cargo run -p typecheck --example typeck -- file.ax`) | — |

## Commands

```bash
cargo test -p typecheck                               # full suite
cargo test -p typecheck --test fuzz                   # fuzz/property tests only
UPDATE_SNAPSHOTS=1 cargo test -p typecheck             # regenerate .thir / .stderr
cargo run -p typecheck --example typeck -- file.ax     # debug THIR dump
cargo run -p cli -- check file.ax                      # CST + HIR + THIR dumps + diagnostics
cargo clippy -p typecheck -- -D warnings              # lint
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