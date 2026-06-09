# Error Handling Redesign: Unified `?` Propagation

> **Status:** Implemented. Replaces the four-keyword design (`try`/`catch`/`?`/`else`)
> with a three-keyword design (`?`/`catch`/`else`). Moved desugaring from
> pre-typecheck to post-typecheck. Resolves DESIGN_SPEC §15 question #8.

## Current state and why we're changing

The current design has four constructs for two kinds of fallibility:

| | Propagate | Default/Handle |
|--|-----------|----------------|
| **Error** `Result<T,E>` | `try expr` | `catch` |
| **Option** `Option<T>` | `expr?` | `else` |

This violates the singular-idiom rule where it hurts most: the developer must
choose between `try` and `?` based on the operand's type *at every call site*.
The type system catches mistakes, but the cognitive load is front-loaded on the
human. Every function boundary where `Option` meets `Result` (most real code)
forces a mode switch.

The previous rationale for keeping them separate was: "they mean different
things." But **the types already encode that meaning.** `?` on `Result` and `?`
on `Option` are semantically identical operations — unwrap success, propagate
failure, no other branches. The distinction (`Err(e)` vs `None`) is in the
return type, not in the operator.

### What changes

| | Old | New |
|--|-----|-----|
| Propagate (Result) | `try expr` | `expr?` |
| Propagate (Option) | `expr?` | `expr?` (same) |
| Default (Result) | `expr catch handler` | `expr catch handler` (same) |
| Default (Option) | `expr else fallback` | `expr else fallback` (same) |

Three constructs. `?` is the universal propagation operator. `catch` and `else`
remain type-specific because handling an error is genuinely different from
handling absence. But propagation is always the same operation: "I don't want
to handle this here."

### `try` keyword removed

The `try` prefix expression is removed entirely. `expr?` replaces it. The
`try` keyword is removed from the lexer (no longer reserved in v1; may be
repurposed later). All fixture code and examples that used `try open(path)`
become `open(path)?`.

### `catch` retains two forms (no change)

```rust
// Bare — discard the error, use a default
val file = open(path) catch default_file()

// Capture — bind the error value
val cfg = read_config(path) catch |e| {
    log("config failed: " + e.to_string())
    default_config()
}
```

No third "catch `{ arms }`" form. `catch { arms }` would create a syntactic
inconsistency where `catch |e| { ... }` is a closure body but `catch { ... }`
is match-arm syntax — same `{ }`, different meaning. When you want to match on
error variants, use `match` explicitly.

### Desugaring moves to after typecheck

**Old pipeline:** source → lex → parse → lower → resolve → **desugar** → typecheck → ownership → rc-insert → IR

**New pipeline:** source → lex → parse → lower → resolve → typecheck → **desugar(? only)** → ownership → rc-insert → IR

Where `catch`/`else`/`ListLit` are desugared in the pre-typecheck pass (in resolver),
and `?` is desugared post-typecheck (in typecheck crate) using `TypeMap`.

This affects all sugar, not just `?`. `ListLit` currently desugars before
typecheck too. Moving it after typecheck is correct — `ListLit` inference
rules are simpler as a first-class node ("elements are `T`, list is `List<T>`")
than as a desugared `List::from(...)` call chain, and the desugar pass produces
the same result either way. Having one desugaring phase instead of two is
simpler, more predictable, and easier to test.

### What doesn't change

- Error sets (Zig-style, structural coercion) — unchanged.
- Error union sugar `E!T` ≡ `Result<T,E>` — unchanged.
- `catch` semantics (Result-only) — unchanged.
- `else` semantics (Option-only) — unchanged.
- `match` on `Result`/`Option` — unchanged.
- `errdefer` status (deferred from v1) — unchanged.

---

## Language surface (new)

```
// ── Option<T> ──
xs.first()?                    // None → return None (propagate)
xs.first() else 0              // None → 0 (default)
xs.first() else compute()      // None → compute() (lazy default)
match xs.first() { ... }       // both branches

// ── Result<T, E> (a.k.a. E!T) ──
open(path)?                    // Err → return Err (propagate)
open(path) catch default        // Err → default
open(path) catch |e| handle(e)  // Err → handler
match open(path) { ... }       // both branches
```

| | Propagate | Default/Handle |
|--|-----------|----------------|
| **Error** `Result<T,E>` | `?` | `catch` |
| **Option** `Option<T>` | `?` | `else` |

Three constructs. `?` for propagation (type-determined). `catch` for error
handling. `else` for absence handling. `match` for exhaustive branching.

---

## Design rationale — what changed and why

### Why `?` unifies Result and Option propagation

1. **Propagation is semantically identical.** `?` on both types does the same
   thing: unwrap the success case, short-circuit the failure case. The only
   difference is whether `Ok(v) => v` or `Some(v) => v`, and the type system
   knows which.
2. **Fewer concepts to learn.** Three operators instead of four. Developers
   never ask "do I use `try` or `?` here?" — they use `?`, and the compiler
   figures out the rest.
3. **The singular-idiom rule is stronger, not weaker.** One way to propagate.
   Two ways to handle (one per type, because handling *is* genuinely different
   between "what went wrong" and "nothing was there").
4. **Rust validated this.** The `Try` trait unifies `?` on `Result` and `Option`
   (and `ControlFlow`, etc.). Axiom doesn't need a trait — it just types `?`
   based on the operand, which is simpler and doesn't require HKT.

### Why `try` is removed (not just deprecated)

`try` occupied the prefix-expression position (`try expr`) for Result
propagation. With `?` unifying both, `try` has no role. Keeping it as a synonym
for `?` on Results would violate the singular-idiom rule — two obvious ways to
do the same thing. Removing it is the clean choice. The keyword is not
repurposed in v1; it may become available for `try` blocks (run-to-first-error
blocks) in a future version if evidence supports that.

### Why `catch` does NOT get `{ arms }` syntax

Consider:
```rust
val cfg = read_config(path) catch |e| { log(e); default() }   // closure body
val cfg = read_config(path) catch { NotFound => default() }   // match arms?
```

Same `{ }`, different meaning. `catch |e| { ... }` is a closure with statements.
`catch { arms }` is match-arm syntax. This is the readability trap Axiom exists
to avoid. When you want to match on error variants, use `match`:

```rust
val cfg = match read_config(path) {
    Ok(v)             => v,
    Err(NotFound)     => default_config(),
    Err(AccessDenied) => panic("no permissions"),
}
```

`catch` always means "provide a fallback for an error." No hidden semantics.

### Why desugaring moves after typecheck

The current design desugars all error-handling sugar before typecheck. This
works because each construct had a fixed desugaring — `try` always produced
`Ok/Err` match arms, `?` always produced `Some/None` match arms. The type was
baked into the desugaring.

With unified `?`, the desugaring **depends on the type of the operand**, which
is only known after typecheck. This forces desugaring to run post-typecheck.

Moving all desugaring (including `ListLit`, `catch`, `else`) to post-typecheck
is the right call because:

1. **One phase, not two.** Previously: `ListLit` and some sugar before typecheck,
   `?`/`catch`/`else` also before typecheck. Now: everything after typecheck.
   Simpler pipeline, easier to reason about.
2. **Same total work.** The typechecker needs inference rules for `?`, `catch`,
   `else`, and `ListLit` whether they're desugared before or after. Direct
   inference on a `?` node is simpler than inferring through expanded `match`
   + fresh variables + exhaustiveness.
3. **No split-brain HIR.** The `Expr` enum carries all variants (sugar +
   core). Typecheck sees sugar variants and infers them. Desugar rewrites them
   to core. Downstream (ownership, rc-insert, IR) gets only core variants and
   hits `unreachable!()` for sugar — catching desugar bugs with a crash rather
   than silent wrong behavior. The same pattern as before, just reorded.

---

## Migration plan — crate by crate

### Lexer (`crates/lexer/`)

| Change | Detail |
|--------|--------|
| Remove `Keyword::Try` | No longer a keyword. `try` becomes an identifier. |
| Remove `SyntaxKind::KwTry` | Generated from keyword table. |
| Update `symbols.rs` | Remove the `("try", Keyword::Try)` mapping. |

**Tests:** Lexer tests that check `try` tokenizes as `KwTry` must be updated to
treat `try` as an identifier. Any fixture using `try` as a keyword must be
rewritten.

### Parser (`crates/parser/`)

| Change | Detail |
|--------|--------|
| Remove `SyntaxKind::TryExpr` | No longer produced by any grammar rule. |
| Remove `K::KwTry` from EXPR_START | `try` is no longer a prefix keyword. |
| Remove `try_expr` grammar rule | The `prefix()` function no longer has a `KwTry` arm. |
| Keep `QuestionExpr` | `?` postfix still produces `QuestionExpr`. Its semantics are now "universal propagation" (not Option-only). |
| Keep `catch_expr`, `else_expr` | Unchanged. |
| Remove `TryExpr` AST type | No CST node, no AST view. |
| Remove `TryExpr::is_prefix()` | Method deleted. |
| Remove `QuestionExpr` → rename? | Consider: `QuestionExpr` is now the *only* propagation node. Name is still accurate (`?` is the question-mark operator). Keep it. |
| Update `expr.rs` grammar | Remove `K::KwTry => Some(prefix(p, K::TryExpr))` arm. `question()` stays. |
| Update `stmt.rs` EXPR_START snippet | Remove `"try x"` from test snippets. |
| Update `is_expr_kind` | Remove `TryExpr`. |
| Update AST coverage test | Remove `TryExpr` from exhaustive match. |

**Fixtures affected:** `error_handling.ax`, `catch_else.ax`, `option_try.ax`,
and any corpus file using `try` as a keyword. All must switch from
`try expr` to `expr?`.

**Error-diagnostic fixture:** `bad_error_handling.ax` must remove the `try`
without-operand error (which no longer exists) and may add `?`-specific errors
if appropriate.

### Lower (`crates/lower/`)

| Change | Detail |
|--------|--------|
| `TryExpr` HIR variant | Rename `Expr::Try(TryExpr)` to `Expr::Question(QuestionExpr)`. Remove `is_option` field — it's always a propagation operator; the typechecker determines Option vs Result. |
| `TryExpr` struct | Rename to `QuestionExpr`. Remove `is_option: bool`. Fields: `id: HirId`, `expr: Box<Expr>`. |
| Remove `lower_try_expr` | No more `try` prefix to lower. |
| `lower_question_expr` | Produces `Expr::Question(QuestionExpr { id, expr })`. No `is_option` field. |
| `lower_expr` dispatch | Remove `TryExpr` arm. `QuestionExpr` arm stays. |
| Serialization | Remove `is_option` label logic (`"OptionTry"` vs `"Try"`). `QuestionExpr` serializes as one label. |
| Invariants test | Remove `TryExpr` from kind lists. `QuestionExpr` stays. |

### Resolver (`crates/resolver/`)

| Change | Detail |
|--------|--------|
| Name resolution | `Expr::Question` — recurse into `expr`. Same as old `Expr::Try` but no `is_option` field. |
| Desugar pass | **Major change.** Moves from resolver (pre-typecheck) to a new location (post-typecheck). See below. |

### Desugar pass — relocation (as implemented)

The desugar pass stayed in `crates/resolver/src/desugar/` for `catch`/`else`/`ListLit`
(pre-typecheck, type-independent). The new `?` desugaring lives in
`crates/typecheck/src/typecheck/question_desugar.rs` and runs post-typecheck via
`check_with_lang_items` in the typecheck module.

**Options:**

1. **Keep in resolver crate, add a second entry point** (`desugar_post_typecheck`)
   that runs after typecheck. Clean but confusing — "why does resolver run
   after typecheck?"
2. **Move to a new `crates/desugar/` crate.** Clean separation, but adds a new
   crate to the workspace for what is currently one file.
3. **Move to existing `crates/lower/` crate** (lower crate already has a
   "CST→HIR lower" and could host "HIR→HIR desugar" as a second pass). Keeps
   the workspace small. The "lowering" concept extends naturally to "lowering
   sugar to core."
4. **Move into `crates/typecheck/`** as a post-inference pass. Typecheck already
   walks all expressions; desugaring is a natural follow-up.

**Recommendation: Option 3.** The lower crate already does one HIR→HIR
transformation (resolving imports). Adding desugar as a second HIR→HIR pass
keeps the pipeline conceptually simple: parse → lower → resolve → typecheck →
desugar → ownership → rc-insert → IR. The lower crate owns all HIR
transformations. The resolver crate owns name resolution, which is a different
concern.

Regardless of crate location, the desugar pass logic changes:

| Old behavior | New behavior |
|-------------|-------------|
| `Expr::Try(e)` with `is_option` field → desugar to `Match` with `Some/None` or `Ok/Err` based on `is_option` | `Expr::Question(e)` → look up the inferred type of the operand from typecheck results → desugar to `Match` with `Some/None` (if `Option<T>`) or `Ok/Err` (if `Result<T,E>`) |
| `Expr::Catch(e)` → desugar to `Match` with `Ok/Err` | `Expr::Catch(e)` → desugar to `Match` with `Ok/Err` (unchanged, but now post-typecheck so type info is available) |
| `Expr::Else(e)` → desugar to `Match` with `Some/None` | `Expr::Else(e)` → desugar to `Match` with `Some/None` (unchanged) |
| `Expr::ListLit(e)` → desugar to `List::from(...)` | `Expr::ListLit(e)` → desugar to `List::with_capacity` + `push` calls (unchanged) |

**Key change for `?`:** The desugar pass needs access to typecheck results to
determine whether `expr?` is `Option` or `Result`. This means the desugar
function signature changes from:

```rust
pub fn desugar(hir: &mut Hir, lang_items: &LangItems, next_id: usize)
```

to something like:

```rust
pub fn desugar(hir: &mut Hir, types: &TypeMap, lang_items: &LangItems, next_id: usize)
```

where `TypeMap` is the typechecker's output mapping `HirId → Ty`. This is a
new dependency but a natural one — desugaring sugar based on inferred types is
exactly what type information is for.

### Typecheck (`crates/typecheck/`)

| Change | Detail |
|--------|--------|
| Remove `unreachable!()` for `Try`/`Catch`/`Else` | These variants now need real inference rules. |
| Add `infer_question` | Infer type of `expr?`: if operand is `Option<T>`, result is `T`, propagation type is `Option<_>`. If operand is `Result<T,E>`, result is `T`, propagation type is `E!_`. Emit error if operand is neither. |
| Add `infer_catch` | Infer type of `expr catch handler`: operand must be `Result<T,E>`. Fallback must be `T`. Result is `T`. |
| Add `infer_else` | Infer type of `expr else fallback`: operand must be `Option<T>`. Fallback must be `T`. Result is `T`. |
| Add `infer_list_lit` | Elements must be same type `T`. Result is `List<T>`. (Currently unreachable; was desugared before typecheck.) |
| `TryInNonErrorFn` diagnostic | Update: `?` is universal now, not Result-only. Diagnostic message: "`?` can only be used in a function that returns `Option<T>` or `E!T`". |
| Serialization | `QuestionExpr` serializes as single label. Remove `"OptionTry"` / `"Try"` branching. |
| Coverage | Update variant label from `"Try"` to `"Question"`. |

### HIR types (`crates/lower/src/hir_types/`)

| Change | Detail |
|--------|--------|
| Remove `TryExpr` struct | Replace with `QuestionExpr` (orrename `TryExpr` → `QuestionExpr`). |
| Remove `is_option` field | Not needed — typechecker determines type. |
| `Expr::Try` variant | Rename to `Expr::Question`. |
| `CatchExpr` | Unchanged. |
| `ElseExpr` | Unchanged. |

### Pipeline order change

From the driver (`crates/driver/` or `crates/cli/`), the compilation pipeline
changes from:

```
parse → lower → resolve → desugar → typecheck → ownership → rc-insert → IR
```

to:

```
parse → lower → resolve → typecheck → desugar → ownership → rc-insert → IR
```

The desugar step moves after typecheck. The driver must pass typecheck results
(`TypeMap`) to the desugar pass.

---

## Test coverage plan

### Correctness invariants (must never break)

1. **After desugar, no sugar variants remain.** A post-desugar scan of the HIR
   must find zero instances of `Expr::Question`, `Expr::Catch`, `Expr::Else`,
   or `Expr::ListLit`. Tested by the existing `test_desugar_*_produces_match`
   tests (adapted to new names and pipeline order).

2. **Desugar is idempotent.** Running desugar twice on the same HIR produces
   identical output. Existing test `test_desugar_is_idempotent` adapted.

3. **`?` on `Option<T>` desugars to `Some/None` match.** Type-aware test:
   parse+lower+resolve+typecheck a function returning `Option<T>`, then desugar,
   then verify the match arms use `Some`/`None`.

4. **`?` on `Result<T,E>` desugars to `Ok/Err` match.** Same, but for `E!T`.

5. **`?` on a non-propagable type is a type error.** `val x: Int = 42?` must
   produce a diagnostic: "`?` can only be used on `Option<T>` or `E!T`."

6. **`catch` on `Option` is a type error.** `xs.first() catch 0` must produce a
   diagnostic: "`catch` can only be used on `Result<T,E>`."

7. **`else` on `Result` is a type error.** `open(path) else default()` must produce
   a diagnostic: "`else` can only be used on `Option<T>`."

8. **Every AST kind is lowered.** The `every_ast_kind_lowered` invariant test
   must include `QuestionExpr` and exclude `TryExpr`.

9. **Golden snapshots.** All `.ast` files for fixtures using `try` must be
   regenerated with `expr?` instead of `try expr`. The `option_try.ax` golden
   already uses `?` (just needs to drop `TryExpr` references).

### New test fixtures

| Fixture | Purpose |
|---------|---------|
| `question_result.ax` | `expr?` on Result-returning function |
| `question_option.ax` | `expr?` on Option-returning function (may merge with `option_try.ax`) |
| `question_error.ax` | Diagnostic: `?` on non-propagable type |
| `catch_on_option_error.ax` | Diagnostic: `catch` on `Option` |
| `else_on_result_error.ax` | Diagnostic: `else` on `Result` |

### Adapted existing fixtures

| Fixture | Change |
|---------|--------|
| `error_handling.ax` | `try open(path)` → `open(path)?` |
| `catch_else.ax` | `try http_get(url)` → `http_get(url)?` |
| `error_try.ax` (corpus) | `try succeed()` → `succeed()?`, `try fail()` → `fail()?` |
| `option_try.ax` | Renamed to `question_propagation.ax` or kept; remove any `TryExpr` references |
| `bad_error_handling.ax` | Remove `try`-without-operand error case; `try` is now an identifier |

### Desugar test relocation

The desugar tests currently live in `crates/resolver/src/desugar/tests.rs` and
test `compile_and_desugar()` which runs the full pipeline through lower →
resolve → desugar. Since desugar now needs typecheck results, the test harness
must be extended to include typecheck:

```rust
fn compile_and_desugar(source: &str) -> Hir {
    // parse → lower → resolve → typecheck → desugar
}
```

This is a test harness change, not a correctness change. The individual test
assertions stay the same (no `Catch` in output, match arms correct, idempotent,
etc.).

---

## Implementation order

```
Step 1: Update HIR types (TryExpr → QuestionExpr, remove is_option)      ✅ done
Step 2: Update parser (remove TryExpr, remove KwTry, remove try grammar)  ✅ done
Step 3: Update lower (remove lower_try_expr, update lower_question_expr)  ✅ done
Step 4: Update typecheck (add inference rules for Question, Catch, Else, ListLit) ✅ done
Step 5: Move desugar from resolver to lower crate (post-typecheck entry)  ✅ done
Step 6: Update desugar (Question desugaring uses TypeMap, remove is_option branching) ✅ done
Step 7: Update driver pipeline (typecheck before desugar, pass TypeMap)    ✅ done
Step 8: Update all fixtures and golden files                              ✅ done
Step 9: Update docs (DESIGN_SPEC, error-handling-plan, README)           🔄 in progress
Step 10: Full test suite: fmt + clippy + test                            ✅ done
```

---

## Summary of removed/added artifacts

**Removed:**
- `Keyword::Try` in lexer
- `SyntaxKind::KwTry` in parser
- `SyntaxKind::TryExpr` in parser
- `TryExpr` AST type in `parser/ast/expr_flow.rs`
- `TryExpr::is_prefix()` method
- `TryExpr` HIR struct in `hir_types/mod.rs`
- `is_option: bool` field on HIR `TryExpr`
- `lower_try_expr()` function in lower
- `desugar_try()` function in desugar (merged into universal `?` desugaring)
- `desugar_option_question()` function in desugar (merged)
- All `TryExpr`/`is_option` references across resolver, typecheck, serialize
- `TryInNonErrorFn` diagnostic (replaced with `QuestionOnNonPropagableType`)
- `try` keyword from Axiom's reserved words

**Added:**
- `QuestionExpr` HIR struct (replaces `TryExpr`, no `is_option` field)
- `infer_question()` in typecheck — real inference rule for `?`
- `infer_catch()` in typecheck — real inference rule for `catch`
- `infer_else()` in typecheck — real inference rule for `else`
- `infer_list_lit()` in typecheck — real inference rule for `ListLit`
- `TypeMap` parameter to desugar pass
- `QuestionOnNonPropagableType` diagnostic
- `CatchOnOptionType` diagnostic
- `ElseOnResultType` diagnostic

**Renamed:**
- `Expr::Try` → `Expr::Question`
- `TryExpr` → `QuestionExpr`
- Desugar functions `desugar_try`/`desugar_option_question` → single `desugar_question`
- `test_desugar_catch_and_else_idempotent` updated (no `try` in source)

**Relocated:**
- Desugar pass: stays in `crates/resolver/src/desugar/` but with post-typecheck `?` desugaring moved to `crates/typecheck/src/typecheck/question_desugar.rs` (called from `check_with_lang_items` after typecheck completes). `catch`/`else`/`ListLit` desugaring stays pre-typecheck in the resolver.