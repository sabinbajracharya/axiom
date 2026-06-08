# Error Handling Implementation Plan — `try`/`else`/`?`/error sets

> Final design: three keywords, zero overlap, all sugar over `match` on enums.
> Based on thorough research of Zig's error handling semantics and the Axiom compiler
> architecture.

## Language Surface

```
// ── Option<T> ──
xs.first()?                    // None → return None (propagate)
xs.first() else 0              // None → 0 (default)
xs.first() else compute()      // None → compute() (lazy default)
match xs.first() { ... }       // both branches

// ── Result<T, E> (a.k.a. E!T) ──
try open(path)                 // Err → return Err (propagate)
open(path) else default        // Err → default
open(path) else |e| handle(e)  // Err → handler
match open(path) { ... }       // both branches
```

| | Propagate | Default |
|--|-----------|---------|
| **Error** `Result<T, E>` | `try` | `else` |
| **Option** `Option<T>` | `?` | `else` |

Three keywords. `try`/`?` unwrap-or-propagate. `else` unwraps-or-falls-back on both.
`match` for exhaustive handling. Zero overlap, zero new parser keywords needed
(`else` already exists in `if`/`else`).

## Architecture Summary

Error handling touches every pipeline stage. Here's what exists and what's needed:

```
             PARSER          LOWER (CST→HIR)     RESOLVER       TYPECHECK         IR/VM
             ======          ================     ========       =========         =====
error set    ✅ CST parsed   ❌ NotYetSupported   ❌ no match    ❌ no Ty variant  N/A
error union  ✅ CST parsed   ❌ NotYetSupported   ❌ no match    ❌ no Ty variant  N/A
try expr     ✅ CST parsed   ❌ UnsupportedExpr   N/A           N/A              N/A
else expr   ✅ CST parsed   ❌ UnsupportedExpr   N/A           N/A              N/A
```

The parser is the only complete layer. Everything below it needs to be built.

Note: The parser CST node `CatchExpr` will be renamed to `ElseExpr` — same shape, better name.
The `catch` keyword in the lexer remains as a reserved word (may be reclaimed later).

---

## Design Rationale — why each decision was made

> This section preserves the reasoning behind the choices so future work
> understands what was considered and rejected, not just what was chosen.

### Why `else` instead of `catch` for defaults

`catch` was the obvious first choice — Zig uses it, and we had a `CatchExpr` CST node.
The problem: `xs.first() catch 0` reads like "catch an error from first" — but `Option`
isn't an error, it's absence. `else` reads correctly for both cases: "open(path) else
fallback" and "xs.first() else 0" both feel natural.

We considered copying Zig's `orelse` for Option defaults (with `catch` for errors).
Rejected: it's a new keyword, and `else` already exists in the parser as part of
`if`/`else` — zero ambiguity since the parser always knows which context it's in.
`else` as a postfix operator is lexically unambiguous (no `if` on the expression stack).

**Alternatives rejected:**
- `catch` for both → wrong semantics on Option
- `orelse` for Option, `catch` for Error → new keyword, copying Zig
- `unwrap_or` method on Option → verbose, inconsistent with `else` on Result

### Why `try` and `?` stay separate

Rust unifies them: `?` works on `Result` and `Option` via the `Try` trait. The spec
considered this (DESIGN_SPEC.md §15, question 8). We chose to keep them separate
because:

1. **They mean different things.** `try open(path)` says "this can fail." `xs.first()?`
   says "this might be absent." Conflating them behind one operator hides a semantic
   distinction the compiler should track — error coercion vs null checking.
2. **`?` earns its brevity.** `?` is a single character after an expression, used on
   nearly every line in Option-heavy code. Replacing it with `else return None` (18 chars)
   adds noise. `try` doesn't need the same compression — error propagation is less
   frequent than Option propagation.
3. **Axiom's rule: one obvious way.** `try` is for errors. `?` is for absence. No
   guessing which one to use.

### Why `errdefer` was dropped

Zig needs `errdefer` because memory is fully manual — every allocation needs a paired
`defer` or `errdefer` written by hand. Axiom has `Deinit` (equivalent to Rust's `Drop`),
which runs automatically on scope exit — resources free themselves.

The remaining use cases for `errdefer` are side-effects (logging, notifications) that
shouldn't live in `Deinit`. These can be handled explicitly at the call site with
`else`: `f() else |e| log(e); return Err(e)`. Since this pattern is rare, adding a
dedicated keyword doesn't earn its keep.

**Revisited if:** evidence shows pervasive `else |e| log(e); return Err(e)` patterns
in real Axiom code.

### Why error sets are unit-only (no data payloads)

Zig's error sets carry no data — just an integer tag. This keeps `try` zero-cost
(a single comparison) and avoids the "error type proliferation" problem where every
error carries a different payload type, making coercion infeasible.

Data-carrying errors use `Result<T, E>` (a stdlib generic enum) where `E` is a
user-defined struct. This separates "what went wrong" (the error tag) from "the
context" (the data payload) — cleaner and more composable.

**Alternatives rejected:**
- Error variants with payloads (like Rust enums) → complicates error set coercion

### Why `else` vs `catch` takes `|e|` capture

`expr else |e| match e { ... }` allows inspecting the error value before deciding the
fallback. This is needed for error handling (match on specific errors) but rarely for
Option (where `None` carries no data). The `|e|` capture syntax mirrors closures and
is opt-in — `expr else fallback` works without it.

### Why error sets use structural coercion (not nominal)

`error{A} → error{A,B}` is implicit (subset → superset). `error{A,B} → error{A}` is a
compile error. This is structural (duck-typed): any error set containing `A`'s tags
is compatible. Nominal coercion (requiring explicit `From` impls like Rust) would add
ceremony for the common case (propagating errors up a call chain) without benefit.

### Why error union sugar `E!T` ≡ `Result<T, E>`

`E!T` in type position is just sugar for `Result<T, E>`. This means no new `Ty` variant
is needed — it's `Ty::Instance("Result", [T, Ty::ErrorSet(...)])`. The `!` is a visual
cue that the function can fail, but it's not a distinct type. This keeps the type system
small and lets `Result` live in the stdlib where it can be extended with combinators.

1. **Error sets are unit-only** (no data payloads). Zig's design is battle-tested and keeps
   the implementation simple.

2. **`try`/`else` desugar to `match` on enums**. `Result<T, E>` and `Option<T>` are
   user-defined generic enums. `try expr` → `match expr { Ok(v) => v, Err(e) => return Err(e) }`.
   `expr else default` → `match expr { Ok/Some(v) => v, Err/None(_) => default }`.
   No new IR ops needed.

3. **`else` is the unified default operator** for both `Option` and `Result`. It replaces
   `catch` for error defaults and works identically on `Option`. Reads naturally:
   "the value, else the fallback."

4. **`?` is Option-only propagation**. Syntactic sugar for `else return None`. Too concise
   and frequent to drop — appears on most lines in Option-heavy code.

5. **`try` is error-only propagation**. Reads as "try this operation" — implies it can fail.
   Kept separate from `?` because errors and absence are different semantics.

6. **`errdefer` is dropped**. With MVS + Perceus + `Deinit`, resource cleanup is automatic
   (like Rust's `Drop`). The remaining use cases (logging, side-effects) are rare enough
   that explicit `else |e| log(e); return Err(e)` at the call site suffices.

7. **Error union sugar `E!T`** ≡ `Result<T, E>`. The `!` in type position desugars to
   `Ty::Instance("Result", [T, Ty::ErrorSet(...)])` — no dedicated `Ty` variant needed.

8. **Error set union `E1 || E2`** creates a new anonymous error set. Coercion is structural:
   `error{A} → error{A,B}` is implicit; `error{A,B} → error{A}` is a compile error.

9. **Error return traces** — deferred to v2 (optional, debug-mode only).

---

## Phase 1: Type System Foundation (HIR + Ty)

### 1a. HIR Item: `ErrorSetDef`

**File: `crates/lower/src/hir_types/items.rs`**

Add to the `Item` enum:
```rust
pub enum Item {
    FnDef(FnDef),
    StructDef(StructDef),
    EnumDef(EnumDef),
    TraitDef(TraitDef),
    ImplDef(ImplDef),
    SubscriptDef(SubscriptDef),
    UseItem(UseItem),
    ErrorSetDef(ErrorSetDef),   // NEW
}
```

New struct (parallels `EnumDef` but simpler — no generics, no payloads):
```rust
pub struct ErrorSetDef {
    pub id: HirId,
    pub name: Name,
    pub visibility: Visibility,
    pub variants: Vec<ErrorVariantDef>,
}

pub struct ErrorVariantDef {
    pub id: HirId,
    pub name: String,
}
```

### 1b. HIR Expression: `Try`, `Else`

**File: `crates/lower/src/hir_types/mod.rs`**

Add to the `Expr` enum:
```rust
pub enum Expr {
    // ... existing variants ...
    Try(TryExpr),     // NEW
    Else(ElseExpr),   // NEW
}

pub struct TryExpr {
    pub expr: Box<Expr>,
}

pub struct ElseExpr {
    pub expr: Box<Expr>,      // the wrapped expression
    pub fallback: Box<Expr>,  // the fallback expression
}
```

### 1d. HIR Type: `ErrorSet`, `ErrorSetUnion`

**File: `crates/lower/src/hir_types/ty.rs`**

Add to `HirTy`:
```rust
pub enum HirTy {
    // ... existing variants ...
    ErrorSet(NameRef),              // named error set: IO
    ErrorSetUnion(Vec<HirTy>),      // union: (E1 || E2)
}
```

`ErrorUnionType` (`E!T`) desugars in lowering to `HirTy::Named("Result")` with type args
`[HirTy::ErrorSet(...), T]` — no dedicated HIR variant needed.

### 1e. Type System: `Ty::ErrorSet`

**File: `crates/typecheck/src/types.rs`**

Add to the `Ty` enum:
```rust
pub enum Ty {
    // ... existing variants ...
    ErrorSet(ErrorSetTy),   // NEW
}

pub struct ErrorSetTy {
    pub name: String,
    pub def_id: DefId,
    pub variants: Vec<String>,  // cached variant names for exhaustiveness
}
```

### 1f. `DefKind::ErrorSet`

**File: `crates/lower/src/lowering/mod.rs`**

Add to `DefKind`:
```rust
pub enum DefKind {
    // ... existing ...
    ErrorSet,        // NEW
    ErrorVariant,    // NEW (or reuse Variant)
}
```

### Phase 1 Tests

| Test | What it proves |
|------|---------------|
| `error_set_def_lowering` | ErrorSetDef CST → HIR produces correct Item::ErrorSetDef |
| `error_set_type_resolves` | Type `IO` resolves to `Ty::ErrorSet(...)` |
| `error_union_desugars` | `IO!Int` lowers to `Instance("Result", [ErrorSet("IO"), Int])` |
| `error_set_union_type` | `(E1 || E2)` lowers to `HirTy::ErrorSetUnion` |
| `try_expr_lowering` | `try f()` lowers to `Expr::Try { expr: Call(...) }` |
| `else_expr_lowering` | `f() else g()` lowers to `Expr::Else { expr, fallback }` |
| `def_kind_error_set` | Lowerer registers `DefKind::ErrorSet` for error set defs |

Fixture files under `crates/lower/tests/fixtures/`:
- `error_set_basic.ax` — simple error set definition
- `error_handling_basic.ax` — try/else with error sets

---

## Phase 2: Lowering (CST → HIR)

### 2a. Lower `ErrorSetDef` items

**File: `crates/lower/src/lowering/item.rs`** (lines ~10-20)

Add to the `lower_item` dispatch:
```rust
K::ErrorSetDef => {
    let def = ast::ErrorSetDef::cast(node).unwrap();
    Ok(Item::ErrorSetDef(lower_error_set_def(ctx, &def)?))
}
```

New function `lower_error_set_def()`:
- Allocate `HirId` for the set
- Push `Def { kind: DefKind::ErrorSet, name: ..., hir_id: ... }` into `ctx.defs`
- Iterate CST `ErrorVariantList` → for each `ErrorVariant`:
  - Allocate `HirId` for variant
  - Push `Def { kind: DefKind::ErrorVariant, ... }`
  - Create `ErrorVariantDef { id, name }`
- Return `ErrorSetDef { id, name, visibility, variants }`

### 2b. Lower `TryExpr`

**File: `crates/lower/src/lowering/expr.rs`**

Replace the catch-all `unsupported_expr` fallthrough for `TryExpr` with explicit lowering.
The `TryExpr` CST node is overloaded — it represents both `try expr` (error propagation)
and `expr?` (Option propagation). Disambiguate by checking the first child token:

```rust
K::TryExpr => {
    let first = node.children().next();
    if first.map_or(false, |c| c.kind() == K::Question) {
        // Postfix `expr?` — Option propagation
        // Kept as UnsupportedFeature for now (v1 scope: error handling only)
        unsupported_expr(ctx, "option propagation (?)", node)
    } else {
        // Prefix `try expr` — error propagation
        let operand = lower_expr(ctx, &???)?;  // the expr child
        Ok(Expr::Try(TryExpr { expr: Box::new(operand) }))
    }
}
```

### 2c. Lower `ElseExpr`

**File: `crates/lower/src/lowering/expr.rs`**

```rust
K::ElseExpr => {
    let ast = ast::ElseExpr::cast(node).unwrap();
    let expr = lower_expr(ctx, &ast.expr().unwrap())?;
    let fallback = lower_expr(ctx, &ast.fallback().unwrap())?;
    Ok(Expr::Else(ElseExpr {
        expr: Box::new(expr),
        fallback: Box::new(fallback),
    }))
}
```

### 2d. Error set def lowering

**File: `crates/lower/src/lowering/ty.rs`**

Handle `ErrorSetUnionType` and `ErrorUnionType`:
- `ErrorSetUnionType`: lower to `HirTy::ErrorSetUnion(members)`
- `ErrorUnionType`: desugar to `HirTy::Named("Result")` with type args `[error_ty, success_ty]`

Named error sets (e.g. `IO` in type position) flow through `HirTy::Named("IO")` naturally —
the resolver will resolve the name to the `ErrorSetDef`.

### Phase 2 Tests

| Test | What it proves |
|------|---------------|
| HIR goldens for `error_set_basic.ax` | ErrorSetDef produces correct HIR structure |
| HIR goldens for `error_handling_basic.ax` | Try/Else produce correct HIR structure |
| `every_ast_kind_lowered` coverage | `TryExpr`, `ElseExpr`, `ErrorSetDef` all lowered |
| Snapshot regeneration | `UPDATE_SNAPSHOTS=1 cargo test -p lower` |
| Desugared error union | Verify `IO!Int` appears as `Named("Result")` in HIR |

---

## Phase 3: Resolution

### 3a. Resolve `ErrorSetDef` items

**File: `crates/resolver/src/resolve/item.rs`**

Add match arm for `Item::ErrorSetDef`:
```rust
Item::ErrorSetDef(e) => resolve_error_set_def(ctx, e),
```

`resolve_error_set_def()` — minimal: error sets have no type params and no payload types,
so there's nothing to resolve in the variants. Just a pass-through (or a no-op). The name
is resolved when referenced in type position via the standard name resolution path.

### 3b. Resolve `Try`/`Else` expressions

**File: `crates/resolver/src/resolve/expr.rs`**

Add match arms for `Expr::Try`, `Expr::Else`:
```rust
Expr::Try(e) => {
    e.expr = resolve_expr(ctx, e.expr)?;
}
Expr::Else(e) => {
    e.expr = resolve_expr(ctx, e.expr)?;
    e.fallback = resolve_expr(ctx, e.fallback)?;
}
```

### 3c. `DefKind` filters

**File: `crates/resolver/src/resolve/mod.rs`**

Add `DefKind::ErrorSet` and `DefKind::ErrorVariant` to the filters in:
- `build_top_level` (line ~151)
- `build_global_exports` (line ~32)

### Phase 3 Tests

| Test | What it proves |
|------|---------------|
| `error_set_name_resolves` | `IO` in type position resolves to the ErrorSetDef |
| `try_operand_resolves` | The expression inside `try` is resolved normally |
| `else_handler_resolves` | The fallback expression is resolved |
| Coverage: all new Expr/Stmt/Item variants handled in resolver | No resolution panics |

---

## Phase 4: Typechecking

### 4a. Collect error set definitions (Pass 1)

**File: `crates/typecheck/src/typeck/collect.rs`**

New method `collect_error_set_defs()` (after `collect_enum_defs`):

```rust
fn collect_error_set_defs(&mut self) {
    let sets: Vec<ErrorSetDef> = self.hir.items.iter()
        .filter_map(|item| match item {
            Item::ErrorSetDef(e) => Some(e.clone()),
            _ => None,
        }).collect();

    for e in &sets {
        let name = e.name.clone();
        let def_id = DefId(e.id.0);
        let mut variants = Vec::new();

        for v in &e.variants {
            let var_name = v.name.clone();
            let var_def_id = DefId(v.id.0);
            variants.push(var_name.clone());

            // Register each variant as a nullary constructor function
            // returning the error set type (like unit enum variants)
            let fn_ty = Ty::Fn(FnTy {
                params: vec![],
                return_type: Box::new(Ty::ErrorSet(ErrorSetTy {
                    name: name.clone(),
                    def_id,
                    variants: vec![], // filled after collection
                })),
            });
            self.env.define(var_name, fn_ty, var_def_id, Mutability::Immutable);
        }

        // Register the error set type
        self.env.define(
            name,
            Ty::ErrorSet(ErrorSetTy { name: name.clone(), def_id, variants: variants.clone() }),
            def_id,
            Mutability::Immutable,
        );
    }
}
```

Call `collect_error_set_defs()` in the collect pipeline (after line 26, before `collect_fn_sigs`).

### 4b. Typecheck `try` (Pass 2)

**File: `crates/typecheck/src/typeck/infer.rs`** or new `error.rs` module

`try expr` desugars to a match on Result:

```rust
// try f()
// desugars to:
// match f() {
//     Ok(v) => v,
//     Err(e) => return Err(e),
// }
```

Implementation:
1. Infer the type of `expr` → should be `Instance("Result", [T, E])` (the error union)
2. If it's `Ok(T)` → yield `T`
3. If it's `Err(E)` → return `Err(E)` from the current function
4. The current function's return type must be `Instance("Result", [R, E])` or compatible
   (error set coercion: E can be a superset of the try'd expression's error)

### 4c. Typecheck `else` (Pass 2)

`expr else fallback`:
1. Infer type of `expr` → `Result<T, E>` or `Option<T>`
2. If `expr` is `Ok(T)`/`Some(T)` → yield `T`
3. If `expr` is `Err(E)`/`None` → evaluate fallback, must produce `T`
4. Result type is `T`

For the `else |e| ...` pattern (error capture), the binding `e` gets type `E` and
the body must produce `T`.

### 4d. Error set coercion

A function `f() -> (E1 || E2)!T` can propagate `E1` errors via `try` — the error set
coerces structurally: `E1`'s values are a subset of `(E1 || E2)`'s values.

Implementation: when checking `return Err(e)` where the function return error set is `S`
and `e` is of error set `E`, verify `E ⊆ S` by checking each variant of `E` exists in `S`.

### 4e. Exhaustiveness on error sets

**File: `crates/typecheck/src/exhaustiveness.rs`**

When the scrutinee is `Ty::ErrorSet`, use the same `check_match_exhaustiveness` machinery.
The error set's `variants` field provides the set of variants to check coverage against.

### Phase 4 Tests

| Test | What it proves |
|------|---------------|
| `error_set_constructor` | `IO.NotFound` typechecks as error set variant |
| `try_ok_propagates` | `try Ok(42)` yields `42 : Int` |
| `try_err_propagates` | `try Err(NotFound)` returns `Err(NotFound)` from function |
| `else_provides_default` | `f() else 0` yields `0` when `f()` returns error |
| `else_handler` | `f() else |e| match e { ... }` typechecks |
| `else_option_default` | `xs.first() else 0` yields `0` when `None` |
| `error_set_coercion_subset` | `fn() -> (E1||E2)!T` can `try` an `E1!T` expression |
| `error_set_coercion_rejected` | `fn() -> E1!T` cannot `try` an `(E1||E2)!T` expression |
| `match_on_error_exhaustive` | Missing error variant reports `NonExhaustiveMatch` |
| THIR goldens | `UPDATE_SNAPSHOTS=1 cargo test -p typecheck` |

Corpus additions under `corpus/valid/`:
- `error_set_basic.ax` — define error set, use variant
- `error_try.ax` — try with Ok and Err paths
- `error_else.ax` — else with default and match handler
- `error_coercion.ax` — subset/superset coercion

Corpus additions under `corpus/errors/`:
- `error_superset_coercion.ax` — compile error: superset → subset
- `error_non_exhaustive_match.ax` — non-exhaustive match on error set
- `error_try_in_non_error_fn.ax` — try in function that doesn't return error union

---

## Phase 5: Desugaring (THIR → THIR)

### 5a. `try` desugaring

**File: `crates/resolver/src/desugar/mod.rs`** (or new pass in typecheck/specialize)

`try expr` → `match expr { Ok(v) => v, Err(e) => return Err(e) }`

This is a THIR-to-THIR transform that runs after typechecking. The desugared form is what
gets lowered to IR. This means the IR layer never sees `Try` — it only sees `Match`.

### 5b. `else` desugaring

`expr else fallback` → `match expr { Ok(v) => v, Err(_) => fallback }` (for Result)
or `match expr { Some(v) => v, None => fallback }` (for Option).

`expr else |e| handler` → `match expr { Ok(v) => v, Err(e) => handler }` (error capture variant).

### 5c. `?` desugaring

`expr?` → `match expr { Some(v) => v, None => return None }`. No new HIR needed —
lowered directly from the parser's `TryExpr` (postfix form) in Phase 2.

### Phase 5 Tests

| Test | What it proves |
|------|---------------|
| `try_desugars_to_match` | After desugaring, THIR contains Match, not Try |
| `else_desugars_to_match` | After desugaring, THIR contains Match, not Else |
| `option_question_desugars` | `expr?` → match with `None => return None` |

---

## Phase 6: IR Lowering + VM Execution

Since `try`/`else`/`?` desugar to `match` on `Result<T, E>` or `Option<T>` (user-defined
generic enums), the IR and VM need **zero new ops** for error handling. The existing
`Match` terminator and `EnumNew`/`VariantPayload` instructions handle everything:

```
try parse_int("42")
// desugars to:
match parse_int("42") {
    Ok(v) => v,
    Err(e) => return Err(e),
}
// lowers to (simplified):
//   %0 = Call parse_int("42")
//   Match %0 [
//     arm Ok(0) => { %1 = VariantPayload(%0, 0); jump merge }
//     arm Err(0) => { %2 = VariantPayload(%0, 0); Return(EnumNew("Result", "Err", [%2])) }
//   ]
// merge:
//   Return(%1)
```

This is already supported by the existing IR instruction set.

### 6a. IR changes needed

**None.** All error handling desugars to `match` on enums, which the IR already supports.

### Phase 6 Tests

| Test | What it proves |
|------|---------------|
| `try_ok_executes` | VM: `try Ok(42)` yields 42 |
| `try_err_propagates_vm` | VM: `try Err(NotFound)` returns Err from function |
| `else_default_vm` | VM: `f() else 0` returns 0 on error |
| IR goldens | `UPDATE_SNAPSHOTS=1 cargo test -p ir` |

---

## Phase 7: Parser Cleanup (minor)

The parser is mostly complete, but a few gaps should be filled:

### 7a. Test fixtures

**File: `crates/parser/tests/fixtures/`**

Create:
- `else_basic.ax` — `expr else fallback` with golden `.ast` snapshot
- `error_set_union.ax` — `(E1 || E2)` in type position with golden `.ast`

### 7b. Fuzz fragments

**File: `crates/parser/tests/fuzz.rs`** (line ~38-43)

Add `"error"` to the `FRAGMENTS` array for fuzz coverage.

### 7c. Disambiguate `TryExpr` overloading (deferred)

The `TryExpr` CST node conflates `try expr` and `expr?`. This is acceptable for now
(the lowerer disambiguates by checking child tokens), but should eventually be split
into `TryExpr` (for error propagation) and `QuestionExpr` (for Option `?`).
**Defer to post-v1 cleanup.**

### Phase 7 Tests

| Test | What it proves |
|------|---------------|
| Golden `.ast` for `else_basic.ax` | ElseExpr CST shape is correct |
| Golden `.ast` for `error_set_union.ax` | ErrorSetUnionType CST shape is correct |
| Fuzz coverage includes `error` | Fuzz exercises error keyword |

---

## Implementation Order

```
Phase 1 (types + HIR)  →  Phase 2 (lowering)  →  Phase 3 (resolution)
                                                    ↓
Phase 7 (parser gaps) ←  Phase 6 (IR/VM)  ←  Phase 5 (desugar)  ←  Phase 4 (typecheck)
```

Each phase is independently testable (golden snapshots + unit tests at each stage).
Phases 1–3 can be developed together (they're tightly coupled). Phase 4 is the biggest
single piece of work — the typechecking logic.

---

## Code Quality Guards

1. **No new `unsafe`** — the workspace `unsafe_code = "forbid"` stays; error handling is
   pure control flow sugar, no FFI needed.

2. **Exhaustive `match` on all new enums** — every `match` on `Expr`, `Stmt`, `Item`,
   `Ty`, `HirTy` covers the new variants. The existing `every_ast_kind_lowered` invariant
   catches missed cases.

3. **One `thiserror` diagnostic enum** — add error-handling-specific diagnostics to the
   existing `TypeDiagnostic` enum: `TryInNonErrorFn`, `ErrorSetSupersetCoercion`,
   `NonExhaustiveErrorMatch`.

4. **Follow existing patterns exactly**:
   - Lower: `lower_error_set_def` mirrors `lower_enum_def`
   - Typecheck: `collect_error_set_defs` mirrors `collect_enum_defs`
   - Desugar: follows existing `ListLit` desugaring pattern
   - Tests: golden snapshots + fixture files + coverage invariants (same as parser/lower)

5. **Per-folder `README.md`** update when files change.

6. **`cargo fmt && cargo clippy -D warnings && cargo test`** green before every commit.

---

## Open Questions (deferred past implementation)

| Question | Why deferred |
|----------|-------------|
| Error return traces | v2 — optional, debug-mode only; not blocking |
| Add `errdefer` back | Dropped for now — `Deinit` covers cleanup, `else` covers side-effects |
| Unify `try` (Result) and `?` (Option) | DESIGN_SPEC.md §15, question 8 |
| `Result` as builtin vs stdlib type | Currently stdlib — revisit if perf requires builtin |
| Error set with data payloads | Zig chose unit-only; Axiom follows. Revisit if evidence shows need |
