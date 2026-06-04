# axiom-hir

The parser's CST/AST views → a desugared, ID-keyed **HIR** where every identifier
resolves to a binding or item def. The third crate of the Axiom compiler, built
test-first against [`docs/hir-testing.md`](../../docs/hir-testing.md).

Three properties define it:
- **ID-keyed.** Every node carries a stable `HirId` for downstream type annotation
  (M2). Name resolution produces `DefId` links from uses to definitions.
- **Desugared.** Trivia is gone; names are resolved (or diagnosed); patterns and
  types are structural, not tree-shaped. This is the layer v1's ownership pass
  will run on.
- **Total.** Every input produces an HIR + diagnostics list, never a panic.
  Unresolved names are `NameRef::Unresolved`; unsupported constructs emit
  `NotYetSupported` diagnostics.

## How it works (end-to-end flow)

```
source → axiom_lexer::lex → axiom_parser::parse → CST
CST → axiom_parser::ast::* → axiom_hir::lower → Hir + diagnostics
```

`lower` is a two-pass pipeline:
1. **Pass 1 — Collect definitions.** Walk top-level items (fn, struct, enum) and
   collect their names into a symbol table. Duplicate definitions are diagnosed.
2. **Pass 2 — Resolve bodies.** Walk expressions and statements, resolving every
   identifier against the symbol table and lexical scopes. Same-scope shadowing
   is disallowed (per `DESIGN_SPEC.md` §8).

## Files

| File | Responsibility | Key items |
|---|---|---|
| `src/lib.rs` | Crate root; public API + `lower` entry + unit tests | `lower`, `serialize`, `HirDiagnostic` |
| `src/hir.rs` | Core HIR types (HirId, NameRef, items, stmts, exprs, patterns, types) | `Hir`, `Item`, `Expr`, `Pattern`, `HirTy` |
| `src/error.rs` | HIR-stage diagnostics (`thiserror`) | `HirDiagnostic` |
| `src/lower/` | Structural lowering (CST → HIR), split by family | `mod.rs` entry |
| `src/lower/mod.rs` | Lower context (ID allocator, diagnostics, defs collection) + entry `lower()` | `LowerCtx`, `DefKind` |
| `src/lower/item.rs` | Item lowering (fn, struct, enum) | `lower_fn_def`, `lower_struct_def`, `lower_enum_def` |
| `src/lower/block.rs` | Block + statement lowering | `lower_block`, `lower_stmt` |
| `src/lower/expr.rs` | Expression lowering (15+ expression kinds) | `lower_expr` + per-kind helpers |
| `src/lower/pattern.rs` | Pattern lowering (7 pattern kinds) | `lower_pattern` + per-kind helpers |
| `src/lower/ty.rs` | Type lowering | `lower_ty` |
| `src/resolve.rs` | Two-pass name resolution | `resolve`, `Scope` |
| `src/serialize.rs` | Canonical HIR dump (pure) | `serialize` |
| `examples/hir.rs` | Debug CLI (`cargo run -p axiom-hir --example hir -- file.ax`) | — |

## Invariants & gotchas

- **Every AST node kind must be handled by the lowerer.** The drift guard test
  `test_lowerer_handles_every_ast_node_kind` (coming in a follow-up) will
  fail at compile time if a new `SyntaxKind` is added without a corresponding
  HIR lowering path.
- **HirIds are assigned in source order** during lowering. This makes the dump
  deterministic and human-readable.
- **Builtins use a reserved HirId range** starting at 1,000,000 to avoid
  collisions with real definitions.
- **Name resolution is per-scope.** Same-scope redefinition is an error;
  nested-scope shadowing is allowed.
- **v0 subset:** fn, struct, enum (no traits, impls, modules, use, const, error
  sets, closures). Unsupported constructs produce `NotYetSupported` diagnostics.

## Commands

```bash
cargo test -p axiom-hir                            # unit tests
cargo run -p axiom-hir --example hir -- file.ax    # debug HIR dump
cargo clippy -p axiom-hir -- -D warnings           # lint
```

## When you change this crate

- Add an HIR node kind: add a variant to the relevant `enum` in `hir.rs`, update
  `serialize.rs`, add the lowering path in `lower/`, add unit tests.
- Add a new name-resolution rule: add a unit test, add a golden fixture.
- Add a new AST node kind upstream (in `axiom-parser`): the lowerer must handle
  it — add the lowering path and a test, or emit a `NotYetSupported` diagnostic.