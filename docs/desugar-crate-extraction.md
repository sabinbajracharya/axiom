# Desugar Crate Extraction — Clean Pipeline Architecture

> **Status:** Proposed. Extracts desugaring into its own crate, making the driver
> the single orchestrator of all pipeline stages. Hides no desugaring inside
> typecheck. Resolves the "hidden desugar" concern.

## The concern this answers

Desugaring is currently split across two crates:

1. `crates/resolver/src/desugar/` — pre-typecheck (catch, else, ListLit)
2. `crates/typecheck/src/typeck/question_desugar.rs` — post-typecheck (?)

The post-typecheck desugaring lives **inside** the typecheck crate, called from
`check_with_lang_items`. This means:

- The driver doesn't control the full pipeline — one desugar step is hidden
- The typecheck crate has a responsibility beyond typechecking
- Testing desugaring requires pulling in the typecheck crate
- The pipeline order is implicit (embedded in function calls) rather than explicit

**The fear:** as more sugar is added (string interpolation, range expressions,
etc.), hidden desugaring inside typecheck becomes a maintenance trap. Future
contributors won't know to look there.

## Current state

### Pipeline (as implemented)

```
driver::check_modules
  ├── parser::parse
  ├── resolver::lower_structural
  ├── resolver::build_global_exports
  ├── resolver::resolve_with_globals
  ├── validate_module_annotations
  ├── resolver::resolve_lang_items
  ├── resolver::desugar::desugar          ← PRE-TYPECHECK (catch, else, ListLit)
  └── typecheck::check_with_lang_items
        ├── collect_pass
        ├── check_pass
        └── question_desugar::desugar_question  ← POST-TYPECHECK (?)
```

### What each desugar does

| What | Phase | Location | Depends on |
|------|-------|----------|------------|
| `ListLit` → `List::new` / `List::with_capacity` + `push` calls | Pre-typecheck | `resolver/src/desugar/mod.rs` | LangItems (resolution info) |
| `catch` → `match { Ok(v) => v, Err(e) => handler }` | Pre-typecheck | `resolver/src/desugar/mod.rs` | None (always Ok/Err) |
| `else` → `match { Some(v) => v, None => fallback }` | Pre-typecheck | `resolver/src/desugar/mod.rs` | None (always Some/None) |
| `?` → `match { Ok/Err ... }` or `match { Some/None ... }` | Post-typecheck | `typecheck/src/typeck/question_desugar.rs` | TypeMap (inferred types) |

### Why the split exists

The split is **correct by necessity**:

- `ListLit` desugaring needs **lang items** (resolved DefIds for `List::new`, `List::push`, etc.). These are available after name resolution.
- `catch`/`else` are **type-independent** — always produce the same match arms regardless of type.
- `?` is **type-dependent** — must determine whether to generate `Some/None` (Option) or `Ok/Err` (Result) arms. Type information is only available after typecheck.

## Proposed architecture

### New crate: `crates/desugar/`

```
crates/desugar/
├── Cargo.toml
├── src/
│   ├── lib.rs              (public API)
│   ├── pre_typecheck.rs    (catch, else, ListLit — needs LangItems)
│   ├── post_typecheck.rs   (? — needs TypeMap)
│   └── helpers.rs          (shared utilities: walk functions, ID generation, temp names)
├── tests/
│   ├── pre_typecheck.rs    (unit tests for catch/else/ListLit desugaring)
│   ├── post_typecheck.rs   (unit tests for ? desugaring)
│   └── fixtures/
│       ├── catch_basic.ax
│       ├── catch_capture.ax
│       ├── else_basic.ax
│       ├── list_empty.ax
│       ├── list_nonempty.ax
│       ├── list_nested.ax
│       ├── question_option.ax
│       ├── question_result.ax
│       └── question_error.ax
└── README.md
```

### New driver pipeline

```
driver::check_modules
  ├── parser::parse
  ├── resolver::lower_structural
  ├── resolver::build_global_exports
  ├── resolver::resolve_with_globals
  ├── validate_module_annotations
  ├── resolver::resolve_lang_items
  ├── desugar::pre_typecheck(&mut hir, &lang_items, next_id)    ← NEW LOCATION
  ├── typecheck::check_with_lang_items(hir, lang_items)         ← NO INTERNAL DESUGAR
  └── desugar::post_typecheck(&mut thir.hir, &thir.types, next_id)  ← NEW LOCATION
```

**Key change:** The driver explicitly calls both desugar phases. The typecheck
crate does no desugaring internally. The pipeline is fully visible in one file.

### Public API

```rust
// crates/desugar/src/lib.rs

/// Pre-typecheck desugaring: catch, else, ListLit.
/// Requires LangItems for List::new / List::with_capacity / List::push.
/// Does NOT touch `?` (needs type information).
pub fn pre_typecheck(
    hir: &mut Hir,
    lang_items: &LangItems,
    next_id: usize,
) -> usize;

/// Post-typecheck desugaring: `?` expressions.
/// Requires TypeMap to determine Option vs Result match arms.
/// Assumes catch/else/ListLit are already desugared.
pub fn post_typecheck(
    hir: &mut Hir,
    types: &TypeMap,
    next_id: usize,
) -> usize;
```

### Dependency graph

```
desugar (new crate)
  depends on:
    - resolver  (Hir, Item, Expr, Block, Stmt, HirId, LangItems, etc.)
    - typecheck (TypeMap, Ty)
    - lexer     (Span)

driver
  depends on:
    - desugar   (new)
    - resolver
    - typecheck

resolver
  depends on: (no change — desugar removed from it)

typecheck
  depends on: (no change — question_desugar removed from it)
```

## Design rationale

### Why a separate crate?

1. **Single responsibility.** The desugar crate does one thing: rewrite sugar to
   core HIR. The resolver does name resolution. The typechecker does type
   inference. Each crate has a clear job.

2. **Driver controls the pipeline.** All orchestration is visible in
   `driver/src/lib.rs`. No hidden desugaring inside function calls. The pipeline
   is: resolve → desugar(pre) → typecheck → desugar(post) → IR.

3. **Testability.** Desugaring can be tested independently without pulling in the
   full typecheck crate. Unit tests can call `pre_typecheck` or `post_typecheck`
   directly.

4. **Extensibility.** When new sugar is added (string interpolation, ranges,
   etc.), it goes into the desugar crate — not scattered across resolver and
   typecheck.

### Why not merge pre and post into one pass?

The two passes have **different dependencies**:

- Pre-typecheck needs `LangItems` (resolution info)
- Post-typecheck needs `TypeMap` (type info)

Merging them would require both dependencies available simultaneously, which
means either:
- Running typecheck twice (wasteful)
- Running desugar inside typecheck (current hidden approach)

Two explicit passes is cleaner.

### Why not keep it in resolver?

The resolver's job is **name resolution**. Desugaring is a separate concern
(rewriting AST/HIR nodes). Mixing them violates single responsibility.

Currently the resolver desugar module is 628 lines. Extracting it frees the
resolver to focus on resolution logic.

### Why not keep question_desugar in typecheck?

The typecheck crate's job is **type inference and checking**. Desugaring is a
downstream transformation that consumes type information but isn't typechecking.

Keeping it in typecheck means:
- The typecheck crate grows with each new sugar
- The typecheck API surface increases (check_with_lang_items returns Thir with
  desugared HIR)
- Testing typechecking requires dealing with desugared output

## Implementation plan

### Phase 1: Create the desugar crate skeleton

**Create `crates/desugar/`:**

| File | Content |
|------|---------|
| `Cargo.toml` | Dependencies: `resolver`, `typecheck`, `lexer` |
| `src/lib.rs` | Re-exports `pre_typecheck` and `post_typecheck` |
| `src/helpers.rs` | Shared walk functions, ID generation, temp name counters |
| `src/pre_typecheck.rs` | Empty — will receive code from resolver |
| `src/post_typecheck.rs` | Empty — will receive code from typecheck |
| `README.md` | Crate description |

**Add to workspace `Cargo.toml`:**
```toml
[workspace]
members = [
    # ... existing ...
    "crates/desugar",
]
```

**Phase 1 tests:** `cargo check -p desugar` compiles.

### Phase 2: Extract shared helpers

Move shared utilities from `resolver/src/desugar/mod.rs` to `desugar/src/helpers.rs`:

| Function | Source | Purpose |
|----------|--------|---------|
| `fresh_id()` | `DesugarCtx.next_id` | Generate unique HirIds |
| `temp_name()` | `DesugarCtx.temp_counter` | Generate temp variable names (`__list_0`, `__q_ok_1`, etc.) |
| `walk_item()` | `desugar_item` | Structural recursion over items |
| `walk_block()` | `desugar_block` | Structural recursion over blocks |
| `walk_stmt()` | `desugar_stmt` | Structural recursion over statements |
| `walk_expr()` | `desugar_expr` (non-sugar arms) | Structural recursion over expressions |
| `walk_match()` | `desugar_match` | Structural recursion over match arms |
| `walk_loop_kind()` | `desugar_loop_kind` | Structural recursion over loop bodies |
| `walk_assign_target()` | `desugar_assign_target` | Structural recursion over assign targets |

**Key decision:** The walk functions are generic — they recurse into all
non-sugar variants and call a callback for sugar variants. This avoids
duplicating the recursion logic in pre_typecheck and post_typecheck.

**Phase 2 tests:** All existing resolver desugar tests pass (using the new helpers).

### Phase 3: Move pre-typecheck desugar

Move from `crates/resolver/src/desugar/mod.rs` to `crates/desugar/src/pre_typecheck.rs`:

| Function | Lines | What it desugars |
|----------|-------|------------------|
| `desugar_catch` | 251-320 | `catch` → `match { Ok/Err }` |
| `desugar_else` | 322-364 | `else` → `match { Some/None }` |
| `desugar_list_lit` | 366-416 | `ListLit` → `List::new` / `with_capacity` + `push` |
| `desugar_empty_list` | 373-393 | `[]` → `List::new()` |
| `desugar_non_empty_list` | 418-459 | `[a,b,c]` → block with `push` calls |
| `desugar_push_call` | 395-416 | Builds `__list.push(elem)` method call |
| `replace_unresolved_name` | 477-573 | Wires catch capture variable to body references |
| `replace_unresolved_name_in_stmt` | 575-592 | Stmt-level wrapper |
| `replace_unresolved_name_in_block` | 594-601 | Block-level wrapper |
| `replace_unresolved_name_in_assign_target` | 603-626 | AssignTarget-level wrapper |

**Also move:**

| Item | Source | Destination |
|------|--------|-------------|
| `DesugarCtx` struct | `mod.rs:15` | `pre_typecheck.rs` (private) |
| `build_result_ok_arm` | `mod.rs:222` | `pre_typecheck.rs` (private) |
| `build_option_some_arm` | `mod.rs:237` | `pre_typecheck.rs` (private) — needed for else |

**Keep in resolver:**
- `desugar/tests.rs` — update imports to use new crate
- `desugar/tests_coverage.rs` — update imports

**Phase 3 tests:**
- All existing resolver desugar tests pass (updated imports)
- `cargo test -p desugar` passes
- No behavior change

### Phase 4: Move post-typecheck desugar

Move from `crates/typecheck/src/typeck/question_desugar.rs` to `crates/desugar/src/post_typecheck.rs`:

| Function | Lines | What it desugars |
|----------|-------|------------------|
| `desugar_question` | 15-37 | Entry point — walks all items |
| `desugar_question_item` | 39-60 | Item-level walk |
| `desugar_question_block` | 62-69 | Block-level walk |
| `desugar_question_stmt` | 71-88 | Statement-level walk |
| `desugar_question_expr` | 90-185 | Core: `?` → match arms based on TypeMap |
| `desugar_question_loop_kind` | 156-168 | Loop body walk |
| `desugar_question_assign_target` | 170-183 | Assign target walk |

**Also move:**

| Item | Source | Destination |
|------|--------|-------------|
| `QuestionDesugarCtx` struct | `question_desugar.rs:8` | `post_typecheck.rs` (private) |
| `build_question_some_arm` | `question_desugar.rs:195` | `post_typecheck.rs` (private) |
| `build_question_ok_arm` | `question_desugar.rs:221` | `post_typecheck.rs` (private) |
| `build_question_none_arm` | `question_desugar.rs:237` | `post_typecheck.rs` (private) |
| `build_question_err_arm` | `question_desugar.rs:256` | `post_typecheck.rs` (private) |

**Phase 4 tests:**
- All existing typecheck question_desugar tests pass (updated imports)
- `cargo test -p desugar` passes
- No behavior change

### Phase 5: Update driver pipeline

**File: `crates/driver/src/lib.rs`**

Replace:
```rust
let max_id = typecheck::hir_max_id(&hir);
resolver::desugar::desugar(&mut hir, &lang_items, max_id + 1);
typecheck::check_with_lang_items(hir, lang_items)
```

With:
```rust
let max_id = typecheck::hir_max_id(&hir);
let next_id = desugar::pre_typecheck(&mut hir, &lang_items, max_id + 1);
let mut thir = typecheck::check_with_lang_items(hir, lang_items);
let max_id = typecheck::hir_max_id(&thir.hir);
desugar::post_typecheck(&mut thir.hir, &thir.types, max_id + 1);
thir
```

**Also update `typecheck::check`** (bare check path):

Remove the internal desugar call. The bare `check` function should either:
- Call `desugar::pre_typecheck` itself, or
- Accept pre-desugared HIR

**Phase 5 tests:** All existing tests pass. No behavior change.

### Phase 6: Clean up old locations

**Remove from resolver:**
- `crates/resolver/src/desugar/mod.rs` — delete the desugar functions (keep tests, update imports)
- `crates/resolver/src/lib.rs` — remove `pub use desugar::desugar` re-export

**Remove from typecheck:**
- `crates/typecheck/src/typeck/question_desugar.rs` — delete entirely
- `crates/typecheck/src/typeck/mod.rs` — remove `mod question_desugar` and the call to `desugar_question`

**Update Cargo.toml:**
- `resolver`: remove `typecheck` dependency if it was only needed for desugar
- `typecheck`: remove `resolver` dependency if it was only needed for desugar
- `desugar`: add dependencies on `resolver` and `typecheck`

**Phase 6 tests:** All tests pass. Clean build with no warnings.

### Phase 7: Add comprehensive tests

**Test harness for desugar crate:**

```rust
// crates/desugar/tests/harness.rs

/// Parse, lower, resolve, and pre-typecheck desugar a source string.
fn compile_and_pre_desugar(source: &str) -> Hir { ... }

/// Parse, lower, resolve, pre-desugar, typecheck, and post-desugar.
fn compile_and_full_desugar(source: &str) -> Thir { ... }

/// Parse, lower, resolve, pre-desugar, typecheck, post-desugar, and serialize.
fn compile_and_serialize(source: &str) -> String { ... }
```

**New test fixtures:**

| Fixture | Purpose |
|---------|---------|
| `catch_basic.ax` | `expr catch fallback` desugars to match |
| `catch_capture.ax` | `expr catch \|e\| handler` wires capture variable |
| `catch_nested.ax` | Nested catch expressions |
| `else_basic.ax` | `expr else fallback` desugars to match |
| `else_nested.ax` | Nested else expressions |
| `list_empty.ax` | `[]` with annotation desugars to `List::new()` |
| `list_nonempty.ax` | `[1,2,3]` desugars to with_capacity + push calls |
| `list_nested.ax` | `[[1],[2,3]]` all lists desugared |
| `list_in_call.ax` | List inside call argument position |
| `list_in_return.ax` | List in return position |
| `question_option.ax` | `?` on Option desugars to Some/None |
| `question_result.ax` | `?` on Result desugars to Ok/Err |
| `question_nested.ax` | Nested `?` expressions |
| `question_in_call.ax` | `?` inside call argument position |
| `mixed.ax` | catch + else + ListLit + ? in same function |

**Golden snapshots:**

| Snapshot | What it captures |
|----------|------------------|
| `pre_desugar/*.hir` | HIR after pre-typecheck desugar (no catch/else/ListLit) |
| `post_desugar/*.hir` | HIR after post-typecheck desugar (no sugar variants) |
| `pre_desugar/*.stderr` | Diagnostics from pre-typecheck desugar |
| `post_desugar/*.stderr` | Diagnostics from post-typecheck desugar |

**Invariant tests:**

| Test | What it proves |
|------|----------------|
| `test_no_sugar_after_pre_desugar` | After pre-typecheck desugar, no `Catch`, `Else`, or `ListLit` in HIR |
| `test_no_sugar_after_post_desugar` | After post-typecheck desugar, no `Question` in HIR |
| `test_pre_desugar_idempotent` | Running pre-typecheck desugar twice produces identical HIR |
| `test_post_desugar_idempotent` | Running post-typecheck desugar twice produces identical HIR |
| `test_all_expr_variants_handled` | Every `Expr` variant is classified as sugar or non-sugar |
| `test_desugar_unique_ids` | Generated HirIds are unique |
| `test_desugar_unique_temp_names` | Generated temp names are unique |

### Phase 8: Documentation

**Update `docs/error-handling-redesign.md`:**
- Update pipeline diagram to show desugar crate
- Remove references to "hidden desugaring inside typecheck"

**Update `docs/error-handling-plan.md`:**
- Update architecture summary table
- Update Phase 5 section

**Create `crates/desugar/README.md`:**
- Crate description
- Module layout
- How to add new sugar

**Update `CLAUDE.md`:**
- Add `crates/desugar/` to the file structure
- Update build/test commands if needed

## Test coverage plan

### Correctness invariants (must never break)

1. **After pre-typecheck desugar, no sugar variants remain** (except `?`). A
   post-pre-desugar scan must find zero instances of `Expr::Catch`, `Expr::Else`,
   or `Expr::ListLit`.

2. **After post-typecheck desugar, no sugar variants remain.** A
   post-post-desugar scan must find zero instances of `Expr::Question`.

3. **Pre-typecheck desugar is idempotent.** Running it twice on the same HIR
   produces identical output.

4. **Post-typecheck desugar is idempotent.** Running it twice on the same HIR
   produces identical output.

5. **Desugar preserves semantics.** A function that compiles cleanly with
   desugared HIR produces the same runtime behavior as the original sugar.

6. **Every AST kind is lowered and desugared.** The
   `every_expr_variant_handled_by_desugar` invariant test must cover all `Expr`
   variants.

7. **Golden snapshots.** All `.hir` files for fixtures must be regenerated with
   the new desugar crate.

### Debugging support

**Serialization format:**

Both `pre_typecheck` and `post_typecheck` produce HIR that serializes cleanly.
The serialization format includes:

- `Desugar` label on desugared match expressions (for debugging)
- Source spans preserved through desugar (for error messages)
- Temp variable names include the sugar they came from (`__list_0`, `__q_ok_1`)

**Diagnostic pruning:**

The pre-typecheck desugar prunes `UnresolvedName` diagnostics for names it
resolved (catch capture variables). This logic moves to the desugar crate but
the diagnostic types stay in the resolver.

**Test output format:**

```rust
// Example test
#[test]
fn test_catch_basic_desugars() {
    let hir = compile_and_pre_desugar("fn f() { val x = g() catch 0 }");
    let dump = serialize(&hir);
    assert!(dump.contains("Match"), "should contain Match: {dump}");
    assert!(!dump.contains("Catch"), "should not contain Catch: {dump}");
    assert!(dump.contains("Ok"), "should have Ok arm: {dump}");
}
```

## Implementation order

```
Phase 1: Create desugar crate skeleton                          ⬜
Phase 2: Extract shared helpers                                 ⬜
Phase 3: Move pre-typecheck desugar from resolver               ⬜
Phase 4: Move post-typecheck desugar from typecheck             ⬜
Phase 5: Update driver pipeline                                 ⬜
Phase 6: Clean up old locations                                 ⬜
Phase 7: Add comprehensive tests                                ⬜
Phase 8: Documentation                                          ⬜
```

Each phase is independently testable. Phases 1-2 are setup. Phases 3-4 are the
core extraction. Phases 5-6 are the switchover. Phases 7-8 are verification.

## Code quality guards

1. **No new `unsafe`** — the workspace `unsafe_code = "forbid"` stays; desugaring
   is pure HIR transformation, no FFI needed.

2. **Exhaustive `match` on all `Expr` variants** — every `match` on `Expr`
   covers all variants. The existing `every_expr_variant_handled_by_desugar`
   test catches missed cases.

3. **One `thiserror` diagnostic enum** — desugar-specific diagnostics (if any)
   go in the desugar crate. Existing diagnostic types stay in their original
   crates.

4. **Follow existing patterns exactly**:
   - Walk functions follow the same structural recursion as resolver desugar
   - Test naming: `test_desugar_<what>_<scenario>`
   - Golden snapshots: `UPDATE_SNAPSHOTS=1 cargo test -p desugar`

5. **Per-folder `README.md`** — update when files change.

6. **`cargo fmt && cargo clippy -D warnings && cargo test`** green before every
   commit.

## Open questions

| Question | Why deferred |
|----------|-------------|
| Should `pre_typecheck` return `usize` (next_id) or take `&mut usize`? | API design — implement first, decide based on usage |
| Should desugar crate depend on `typecheck` for `TypeMap`, or receive it as a trait? | Trait adds indirection; start with direct dependency |
| Should we add string interpolation desugaring to this crate now? | Defer until string interpolation design is finalized |
| Should the bare `check()` path call desugar directly or receive pre-desugared HIR? | Depends on how bare check is used in tests |
