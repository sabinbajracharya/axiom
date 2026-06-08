# Pipeline & Crate Restructure — Design Plan

> **Status: Implemented — 2025-06-08.** Companion docs: `DESIGN_SPEC.md`, `docs/intrinsic-and-stdlib-identity.md`,
> `docs/stdlib-loading-unification.md`, `ENFORCEMENT.md`, `RUST_CONVENTIONS.md`.

## 0. The concern this answers

Three structural problems in the current crate/pipeline design that caused real pain during
implementation of `@intrinsic` validation:

1. **Two diagnostic vectors with the same field name.** `Thir.diagnostics` (`Vec<TypeDiagnostic>`)
   and `Thir.hir.diagnostics` (`Vec<HirDiagnostic>`) live at adjacent nesting levels, holding
   different types. This caused a bug where the `IntrinsicOutsideStdlib` e2e test checked the wrong
   vector — and passed silently with zero diagnostics instead of failing visibly. The architecture
   invites this mistake.

2. **`check_modules` lives in `axiom-typeck` but parses and lowers HIR.** The type-check crate
   depends on `axiom-parser` and calls `axiom_parser::parse()` directly. It also calls
   `axiom_hir::lower_structural()`, `axiom_hir::resolve_with_globals()`, and
   `axiom_hir::build_global_exports()`. The type checker is a pipeline driver, not a pure checker.

3. **Lowering and resolution share one crate (`axiom-hir`) and one type (`Hir`).** The same
   `Hir` struct is used before and after name resolution. You can accidentally pass an unresolved
   tree to the type checker because there is no type-level distinction between "before resolve"
   and "after resolve."

These are pre-1.0 architectural debt. The language is in v0 — fixing them now is cheap;
fixing them later multiplies the blast radius across snapshot files, test infrastructure, and
downstream tooling.

## 1. The problem, stated plainly

### 1a. Two diagnostic vectors, one name

```rust
pub struct Thir {
    pub hir: axiom_hir::Hir,           // hir.diagnostics: Vec<HirDiagnostic>
    pub types: TypeMap,
    pub diagnostics: Vec<TypeDiagnostic>,  // ← same field name, different type
}
```

When writing `thir.diagnostics.iter().any(...)`, the compiler points you at `Vec<TypeDiagnostic>`.
`IntrinsicOutsideStdlib` and `LangItemOutsideStdlib` are `HirDiagnostic` variants — they live in
`thir.hir.diagnostics`, a vector two levels deep with the identical field name.

**37 call sites** across test files access `thir.diagnostics` expecting type-check errors. Exactly
**2 call sites** know to look at `thir.hir.diagnostics` for HIR-level errors. The CLI manually
iterates both vectors in two separate loops (`cli/src/check.rs:66-70`, `cli/src/lib.rs:129-140`).
VM test helpers check both with combined `is_empty()` guards (`vm/tests/*_e2e.rs`).

No single `all_diagnostics()` method exists. Every consumer must know which phase produced
which error category.

### 1b. The type checker parses source code

`axiom-typeck/src/lib.rs:48-107` (`check_modules`) does:

```rust
for (name, source) in modules {
    let result = axiom_parser::parse(source);              // ← depends on axiom-parser
    let Some(root) = axiom_parser::ast::SourceFile::cast(result.tree) else { continue; };
    let (items, defs, diags, nid) = axiom_hir::lower_structural(&root, source, next_id);
    ...
}
```

This means `axiom-typeck` must depend on `axiom-parser`. It also depends on `axiom-stdlib` (for
`is_stdlib_module()`). The type checker is a pipeline orchestrator, not a pure type-checking pass.

In a greenfield design, you pass a resolved `Hir` into `check()` and get a `Thir` back. The
multi-module orchestration lives above the checker, not inside it.

### 1c. One `Hir` for two phases

`Hir` is produced by `lower_structural()` with `NameRef::Unresolved("name")` and then mutated
in-place by `resolve_with_globals()` to `NameRef::Resolved(DefId(42), "name")`. The invariant
"all names are resolved" is enforced by convention and the coverage invariant (`check_all`),
not by the type system. If you forget to call `resolve_with_globals`, the type checker still
compiles — and produces subtle wrong results.

## 2. The solution

### 2a. Unified diagnostic surface

Remove `diagnostics` from the `Hir` struct. Every phase returns diagnostics as a separate value.
The `Thir` folds all prior-phase diagnostics into one vector at construction time.

```rust
// NEW
pub enum Diagnostic {
    Lower(LowerDiagnostic),
    Resolve(ResolveDiagnostic),
    Desugar(DesugarDiagnostic),
    Type(TypeDiagnostic),
}

pub struct Thir {
    pub hir: Hir,
    pub types: TypeMap,
    pub diagnostics: Vec<Diagnostic>,  // ← one vector, all phases
}
```

`Diagnostic` gets a single `kind()` method that works regardless of which phase produced the error.
No consumer ever asks "was this a lower error or a type error?" unless it explicitly
pattern-matches the enum variant.

`Hir.diagnostics` is deleted. The HIR is a pure tree.

### 2b. New `driver` crate — pipeline orchestrator

Move `check_modules`, `check_source`, and `validate_module_annotations` to a new `driver` crate
that sits between `typecheck` and `cli`:

```
                  driver
                 /   |   \
          lower  resolve  desugar  typecheck  stdlib
             \    |     /        /
              \   |    /        /
               parser          /
                  \           /
                   lexer
```

`driver` depends on everything. `cli` depends on `driver`. No other crate depends on `driver`.
Test crates dev-depend on `driver` instead of `typecheck`.

`axiom-typeck` no longer depends on `axiom-parser` or `axiom-stdlib`. It becomes a pure
function: resolved `Hir` + `LangItems` → `Thir`.

### 2c. Split `axiom-hir` into `lower` + `resolve`

Two crates, two distinct output types:

| Crate | Output type | Names are |
|-------|-----------|-----------|
| `lower` | `LoweredHir` | `NameRef::Unresolved("...")` |
| `resolve` | `Hir` | `NameRef::Resolved(DefId, "...")` |

The type checker only accepts `Hir` (resolved). You cannot accidentally pass an unresolved tree.

`Desugar` stays in `resolve` or becomes its own crate. It modifies `Hir` in place and has no
separate output type — but the handoff is explicit in the pipeline.

### 2d. Drop `axiom-` prefix, fix cryptic names

| Old | New | Reason |
|-----|-----|--------|
| `axiom-lexer` | `lexer` | Workspace scopes them; prefix is future-hostage to name change |
| `axiom-parser` | `parser` | Same |
| `axiom-hir` | split into `lower` + `resolver` | Two phases, two crates |
| `axiom-typeck` | `typecheck` | No abbreviation; clearer to non-compiler-devs |
| `axiom-mono` | `specialize` | "mono" is cryptic; "specialize" describes the outcome |
| `axiom-ir` | `ir` | Standard |
| `axiom-vm` | `vm` | Standard |
| `axiom-cli` | `cli` | Standard |
| `axiom-stdlib` | `stdlib` | Standard |
| `axiom-modules` | `modules` | Standard |
| — (new) | `driver` | Pipeline orchestrator |

### 2e. Scope chain: HashMap clone → parent reference

While splitting `resolve` into its own crate, change `Scope` from a HashMap-clone-on-enter to a
linked-list-with-parent-reference. This is not load-bearing (blocks are max ~5 deep), but it's
correct architecture for when closures arrive (Spike 0). One struct change:

```rust
pub(crate) struct Scope<'a> {
    parent: Option<&'a Scope<'a>>,
    own_names: HashMap<String, (DefId, DefKind)>,
}
```

Lookups chain up via `parent`. No cloning. No drop/pop explicit — the borrow checker handles it.

## 3. Implementation phases

Each phase is one commit. All tests must pass, `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`
at the end of each phase.

### Phase 1: Unified Diagnostic enum (in `axiom-typeck`) ✅ DONE

**Scope:** `axiom-typeck` only. No crate splits yet.

1. Rename `TypeDiagnostic` → `TypeError`.
2. Create `Diagnostic` enum with variants `Lower(HirDiagnostic)`, `Type(TypeError)`.
3. Add `HirDiagnostic::kind()` method (mirrors `TypeError::kind()`).
4. Add `Diagnostic::kind()` delegating to inner variant's `kind()`.
5. Change `Thir.diagnostics` from `Vec<TypeDiagnostic>` to `Vec<Diagnostic>`.
6. Fold `hir.diagnostics` into `Thir.diagnostics` in `check_with_lang_items` — remove the
   redundant vector.
7. Update all 37+ test call sites that check `thir.diagnostics` to use the new enum.
8. Update CLI diagnostic rendering to use one loop.
9. Rebuild golden snapshots (147 files touched, format unchanged — Diagnostic::kind() output
   matches current format).
10. Update coverage invariants — `check_all` now checks the unified vector.

**Gate:** All tests pass. `thir.hir.diagnostics` no longer exists — verified by grep.
`thir.diagnostics` is the single source.

### Phase 2: Extract `driver` crate ✅ DONE

**Scope:** New crate `driver`. `axiom-typeck` loses parser/stdlib deps.

1. Create `crates/driver/` with empty `Cargo.toml` depending on: `axiom-lexer`, `axiom-parser`,
   `axiom-hir`, `axiom-typeck`, `axiom-stdlib`.
2. Move `check_modules`, `check_source`, `validate_module_annotations`, `is_stdlib_module` from
   `axiom-typeck/src/lib.rs` → `driver/src/lib.rs`.
3. `axiom-typeck` retains: `check`, `check_with_lang_items`, `serialize`, `check_all`,
   `monomorphize`, `Thir`, `Ty`, `TypeMap`, `Diagnostic`.
4. Remove `axiom-stdlib` from `axiom-typeck` Cargo.toml. `axiom-parser` remains (used by prelude IO parsing in collect.rs).
5. Update all 37+ call sites from `axiom_typeck::check_modules` → `driver::check_modules`.
6. Update `axiom-cli` Cargo.toml to depend on `driver` (and remove direct `axiom-typeck` dep if
   it only used it for `check_modules`).
7. `axiom-ir` dev-deps: add `driver`, keep `axiom-typeck` (it still needs `Thir` types for tests).
8. All VM test Cargo.toml files: add `driver` dev-dependency.
9. Update golden test pipeline calls in test helpers.

**Risk:** `axiom-ir` already dev-depends on `axiom-typeck`. Adding `driver` as a dev-dep means
`driver → axiom-typeck` and `axiom-ir (dev) → driver`. No cycle — `axiom-ir` is not a dependency
of `driver`. But `axiom-ir`'s Cargo.toml dev-deps section grows by one entry.

**Gate:** `cargo tree -p axiom-typeck` shows no `axiom-stdlib` in the regular
dependency graph. `axiom-parser` remains (needed for prelude IO). All tests pass.

### Phase 3: Split `axiom-hir` into `lower` + `resolver` ✅ DONE

**Scope:** Two new crates. `axiom-hir` becomes a re-export shim (temporary).

1. Create `crates/lower/` Cargo.toml: depends on `axiom-lexer`, `axiom-parser`, `thiserror`.
2. Create `crates/resolver/` Cargo.toml: depends on `lower`, `axiom-lexer`, `thiserror`.
3. Move `axiom-hir/src/lower/` → `lower/src/`.
4. Move `axiom-hir/src/resolve/` → `resolver/src/`.
5. Move `axiom-hir/src/error.rs` (HirDiagnostic) → split: `LowerDiagnostic` stays in `lower`,
   `ResolveDiagnostic` stays in `resolver`. Shared diagnostic variants (if any) become a
   `DiagnosticCommon` type in `lower` that `resolver` re-exports.
6. Move `axiom-hir/src/intrinsic.rs` → `resolver/src/intrinsic.rs` (it's phase-agnostic key
   registry, but used during resolve-phase annotation validation).
7. Move `axiom-hir/src/lang.rs` → `resolver/src/lang.rs`.
8. Move `axiom-hir/src/desugar/` → `resolver/src/desugar/` (runs after resolve, before typeck).
9. Move `axiom-hir/src/hir/` → shared types. Split: base types (`Hir`, `Item`, `HirId`, `DefId`,
   `NameRef`, `Expr`, `Block`, `Stmt`, etc.) become `lower::hir` module. `resolver` re-exports.
10. Move `GlobalExports`, `build_global_exports` → `resolver/src/exports.rs`.
11. `axiom-hir` crate remains but becomes a re-export façade: `pub use lower::*; pub use resolver::*;`
    to avoid breaking every downstream crate at once.
12. Update `driver` Cargo.toml to depend on `lower` + `resolver` instead of `axiom-hir`.
13. Update `typecheck`, `ir`, `vm`, `cli` Cargo.toml to depend on `lower` + `resolver` (or keep
    the `axiom-hir` façade).
14. Drift-guard tests (intrinsic.rs:138, lang.rs:339) updated with new crate root names.

**Risk:** The `axiom-hir` re-export façade creates a transition period where both old and new
paths exist. This is deliberate — it lets us phase the migration without one mega-commit.
Delete the façade in a follow-up once all consumers have migrated.

**Gate:** `cargo tree` shows `axiom-hir` depends only on `lower` + `resolver` (re-export only).
All snapshot comparisons match pre-split output.

### Phase 4: Rename crates ✅ DONE

**Scope:** Mechanical rename. No code changes except string literals and Cargo.toml `[package] name`.

1. Rename directories:
   - `crates/axiom-lexer/` → `crates/lexer/`
   - `crates/axiom-parser/` → `crates/parser/`
   - `crates/axiom-hir/` → `crates/hir/` (temporary, then deleted in Phase 5)
   - `crates/axiom-typeck/` → `crates/typecheck/`
   - `crates/axiom-mono/` → `crates/specialize/` (or stays `axiom-typeck/src/mono/` —
     see Phase 5)
   - `crates/axiom-ir/` → `crates/ir/`
   - `crates/axiom-vm/` → `crates/vm/`
   - `crates/axiom-cli/` → `crates/cli/`
   - `crates/axiom-stdlib/` → `crates/stdlib/`
   - `crates/axiom-modules/` → `crates/modules/`
   - `crates/driver/` → `crates/driver/` (already unprefixed)
2. Update `[package] name` in every Cargo.toml.
3. Update `[dependencies]` in every Cargo.toml.
4. Update `[workspace] members` in root Cargo.toml.
5. Update string literals in drift-guard tests (intrinsic.rs:138, lang.rs:339, fixture_coverage.rs:46).
6. Update path references in `axiom-stdlib/build.rs` (walks `../stdlib/`).
7. Update CLI error messages that mention crate names (if any).
8. `sed` or a script for mechanical replacement of `axiom_lexer::` → `lexer::` etc. across all
   `.rs` files.

**Gate:** `cargo build --workspace` succeeds. `cargo test --workspace` passes. Zero occurrences
of the string "axiom-" in Cargo.toml files (except workspace package name).

### Phase 5: Extract `specialize` crate, scope chain refactor, final cleanup ✅ DONE

**Status:** Everything complete. 5.4 (scope chain) deferred — 20+ cascading call sites,
better done as a standalone change when closures arrive (Spike 0).

**Scope:** Optional refinements that can be deferred.

1. Move `axiom-typeck/src/mono/` to its own `crates/specialize/` crate. It depends on
   `typecheck` (reads `&Thir`), is consumed by `ir`. This is optional — keeping it in
   `typecheck` is also correct since mono is a post-typeck pass that only reads `&Thir` types.

2. Scope chain refactor: change `Scope::new_child()` from HashMap clone to parent reference.
   Lives in `resolver/src/scope.rs`.

3. Delete the `hir` re-export façade crate once all consumers have migrated to `lower` +
   `resolver` directly.

4. Move `axiom-typeck/src/typeck/mod.rs:57` desugar call into `driver`'s pipeline — desugar
   runs between resolve and typeck, which is the driver's job, not the type checker's.

5. Optional: extract `DesugarResult.next_id` usage — currently returned but never consumed by
   `check_with_lang_items` (line 57 discards the return). Either remove the wrapper or
   document it as forward-compatible.

**Gate:** All tests pass. `cargo clippy --workspace --all-targets -- -D warnings`. Golden
snapshots regenerated.

## 4. Migration safety

### 4a. No big-bang refactor

Phases 1–5 are ordered so that each phase leaves the codebase in a working, test-passing state.
No phase depends on a future phase being completed. You can stop after Phase 2 (driver crate)
and the benefit is already realized — `axiom-typeck` is pure, diagnostics are unified.

### 4b. Re-export façade for Phase 3

`axiom-hir` becomes `pub use lower::*; pub use resolver::*;` — a thin re-export crate that
preserves all existing import paths. Downstream crates migrate from `axiom_hir::` to
`lower::` / `resolver::` at their own pace. The façade is deleted only after every consumer
has migrated.

### 4c. Golden snapshot handling

Each phase regenerates golden snapshots via `UPDATE_SNAPSHOTS=1 cargo test`. The format
changes only in Phase 1 (Diagnostic kind strings may shift slightly). Phases 2–5 are pure
reorganization — the serialization output is identical.

### 4d. Hardcoded crate name strings — audit

These string literals must be updated during Phase 4 and verified:

| File | Line | String | Action |
|------|------|--------|--------|
| `intrinsic.rs` | 138 | `"axiom-hir"`, `"axiom-typeck"`, `"axiom-ir"`, `"axiom-vm"` | Replace with new crate names |
| `intrinsic.rs` | 142 | `"crates/axiom-hir → repo root"` | Update path |
| `lang.rs` | 339 | Same four crate names | Replace |
| `lang.rs` | 344 | Same expect message | Update |
| `fixture_coverage.rs` | 46 | `.strip_prefix("axiom-")` | Replace with new prefix logic |
| `stdlib/build.rs` | — | Walks `stdlib/` relative to workspace | Verify path still resolves |

## 5. Testing strategy

### 5a. Per-phase gate

After every phase (each a single commit):

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

All three must pass. No exceptions.

### 5b. No regressions in golden snapshots

The serialization format changes only in Phase 1 (Diagnostic::kind()). All 147 golden files
are regenerated with `UPDATE_SNAPSHOTS=1`. A separate commit after Phase 1 captures the
regenerated snapshots so the diff is reviewable.

### 5c. Coverage invariants preserved

All five `check_all` functions continue to work:
- `lexer::check_all` — tiling + reconstruction (unchanged)
- `parser::check_all` — token-to-tree consistency (unchanged)
- `lower::check_all` — unresolved-name → diagnostic coverage (unchanged)
- `typecheck::check_all` — updated to check unified Diagnostic vector instead of
  TypeDiagnostic vector
- `ir::check_invariants` — register/block validity (unchanged)

### 5d. Cross-crate dependency verification

After each phase, run:

```bash
cargo tree -p typecheck          # Phase 2: verify no axiom-parser dep
cargo tree -p lower --invert     # Phase 3: verify who depends on lower
cargo tree --workspace --depth 1 # Phase 4: verify no axiom- prefix in crate names
```

## 6. Out of scope (deferred)

- **Closure capture / Perceus implementation** — Spike 0 work. The scope chain refactor
  (Phase 5, item 2) makes the resolve pass closure-ready but does not implement closures.
- **ParseError unification** — Parse errors are rendered at the call site (CLI) before
  `Thir` exists. They stay as `Vec<ParseError>` for now. A future phase could fold them
  into `Diagnostic` at the driver level, but it's not load-bearing for the confusion fix.
- **`format` builtin de-specialization** — Currently a 3-layer special case (resolve + typeck
  + VM). Making it a normal `@intrinsic` or `@lang` item is deferred.
- **Self-hosting** — Not relevant until v2.x.

## 7. Decisions and open questions

1. **Q: Does `specialize` need its own crate?** A: Optional. Keeping mono in `typecheck` is fine
   — it reads `&Thir` and produces `MonoResult`, both defined in `typecheck`. The only
   advantage of a separate crate is discipline (nobody accidentally calls typeck from mono).

2. **Q: Where does desugar live — `resolver` or its own crate?** A: In `resolver`. Desugar runs
   after resolution but before type checking. It takes a resolved `Hir` and `LangItems` and
   mutates the tree. It belongs with the resolution machinery.

3. **Q: Should `check_with_lang_items` move to `driver`?** A: No. It's a pure function:
   `Hir + LangItems → Thir`. The driver orchestrates — it calls `check_with_lang_items`, but
   the function itself stays in `typecheck`.

4. **Q: Do we keep `Hir.diagnostics` during the transition?** A: Phase 1 removes it. The
   HIR unit tests that currently check `hir.diagnostics` get updated to receive diagnostics
   as separate values from `lower_structural`.

5. **Q: `check_modules` has 37 call sites. Worth it?** A: Yes — sed-level mechanical change.
   `s/axiom_typeck::check_modules/driver::check_modules/g` across test files.

## 8. Commit plan

| Commit | Message | Scope |
|--------|---------|-------|
| 1 | `refactor: unified Diagnostic enum, Hir.diagnostics removed` | ~100 lines changed, golden regen |
| 2 | `refactor: extract driver crate, typecheck no longer depends on parser` | New crate + 37 call site updates |
| 3 | `refactor: split axiom-hir into lower + resolver crates` | Two new crates + re-export façade |
| 4 | `refactor: rename crates — drop axiom- prefix, typeck→typecheck, mono→specialize` | Mechanical rename, ~50 files |
| 5 | `refactor: extract specialize crate, scope chain parent-ref, delete hir façade` | Optional cleanup |
