# Error Handling Implementation Plan — `?`/`catch`/`else`/error sets

> **Updated design:** three constructs, zero overlap, all sugar over `match` on enums.
> `?` is the universal propagation operator (both Result and Option).
> `catch` for Result/error defaults, `else` for Option defaults.
> `try` keyword has been **removed** (see [`error-handling-redesign.md`](error-handling-redesign.md)).
> `errdefer` is **deferred** — MVS + Perceus + `Deinit` handles cleanup; `catch`/`else`
> handle side-effects. Revisit only with evidence.

## Language Surface

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

| | Propagate | Default |
|--|-----------|---------|
| **Error** `Result<T, E>` | `?` | `catch` |
| **Option** `Option<T>` | `?` | `else` |

Three constructs. `?` for propagation (type-determined — Option or Result).
`catch` for error defaults, `else` for option defaults. `match` for
exhaustive handling. Zero overlap. `try` has been removed — `?` is the
universal propagation operator.

## Architecture Summary

Error handling touches every pipeline stage. Here's what exists and what's needed:

```
             PARSER          LOWER (CST→HIR)     RESOLVER       TYPECHECK         IR/VM
             ======          ================     ========       =========         =====
error set    ✅ CST parsed   ✅ HIR type         ✅ resolved     ✅ collected      N/A
error union  ✅ CST parsed   ✅ HIR type         ✅ resolved     ✅ resolved       N/A
? expr       ✅ CST parsed   ✅ HIR QuestionExpr ✅ resolved     ✅ inferred       N/A
catch expr   ✅ CST parsed   ✅ HIR CatchExpr    ✅ resolved     ✅ inferred       N/A
else expr    ✅ CST parsed   ✅ HIR ElseExpr     ✅ resolved     ✅ inferred       N/A
errdefer     ✅ parsed (scaffolded, not in spec) — deferred from v1
desugar      N/A             N/A                ✅ catch/else/ListLit → match (pre-typecheck)
             N/A             N/A                N/A             ✅ ? → match (post-typecheck)
```

All error-handling expressions are desugared to `match` on enums during name
resolution, before typechecking. Typecheck never sees `Try`/`Catch`/`Else` —
its `infer_expr` arms for those are `unreachable!` safety nets.

Note: `CatchExpr` (error default) and `ElseExpr` (option default) are separate CST + HIR nodes.
The `catch` keyword is a full first-class keyword (not just reserved).

---

## Design Rationale — why each decision was made

> This section preserves the reasoning behind the choices so future work
> understands what was considered and rejected, not just what was chosen.

### Why `catch` for errors, `else` for options

We follow Zig's two-keyword approach: `catch` for Result/error defaults, `else`
for Option/null defaults. This keeps the semantics explicit — `catch` implies an
error context, `else` implies absence.

**Alternatives rejected:**
- `else` for both → conflates two semantically different operations
- `catch` for both → wrong semantics on Option (absence isn't an error)
- `orelse` for Option → new keyword, verbose, Zig-specific terminology

`catch` feels natural for errors ("catch the error and use this fallback").
`else` feels natural for options ("some value, else this default").
`else` already exists in the parser as part of `if`/`else` — zero ambiguity
since the parser always knows which context it's in.

### Why `?` unifies Result and Option propagation

`try` and `?` were originally separate: `try` for Result, `?` for Option. This was
revised because:

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

See [`error-handling-redesign.md`](error-handling-redesign.md) for the full rationale.

### Why `errdefer` is deferred

Zig needs `errdefer` because memory is fully manual — every allocation needs a paired
`defer` or `errdefer` written by hand. Axiom has `Deinit` (equivalent to Rust's `Drop`),
which runs automatically on scope exit — resources free themselves.

The remaining use cases for `errdefer` are side-effects (logging, notifications) that
shouldn't live in `Deinit`. These can be handled explicitly at the call site with
`catch`: `f() catch |e| { log(e); return Err(e) }`. Since this pattern is rare, adding a
dedicated keyword doesn't earn its keep.

**Revisited if:** evidence shows pervasive `catch |e| { log(e); return Err(e) }` patterns
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

### Why `catch |e|` takes error capture

`expr catch |e| handler` allows inspecting the error value before deciding the
fallback. This is needed for error handling (match on specific errors) but not
for Option (where `None` carries no data — `else` never takes a capture).
The `|e|` capture syntax mirrors closures and is opt-in —
`expr catch fallback` works without it.

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

2. **`?`/`catch`/`else` desugar to `match` on enums**. `Result<T, E>` and `Option<T>` are
   user-defined generic enums. `expr?` → `match expr { Ok(v) => v, Err(e) => return Err(e) }`
   (or `Some(v) => v, None => return None` for Option — determined by typecheck).
   `expr catch fallback` → `match expr { Ok(v) => v, Err(_) => fallback }`.
   `expr catch |e| handler` → `match expr { Ok(v) => v, Err(e) => handler }`.
   `expr else fallback` → `match expr { Some(v) => v, None => fallback }`.
   No new IR ops needed.

3. **`catch` is error-only; `else` is Option-only.** No overlap — the singular-idiom rule.
   `catch` says "the operation can fail." `else` says "the value might be absent."
   Using `catch` on an `Option` or `else` on a `Result` is a type error.

4. **`?` is the universal propagation operator.** Works on both `Result` and `Option`.
   The typechecker determines the match arms (`Ok/Err` vs `Some/None`).
   `try` has been removed — three keywords instead of four.

5. **`errdefer` is deferred**. With MVS + Perceus + `Deinit`, resource cleanup is automatic
   (like Rust's `Drop`). The remaining use cases (logging, side-effects) are rare enough
   that explicit `catch |e| { log(e); return Err(e) }` at the call site suffices.

6. **Error union sugar `E!T`** ≡ `Result<T, E>`. The `!` in type position desugars to
   `Ty::Instance("Result", [T, Ty::ErrorSet(...)])` — no dedicated `Ty` variant needed.

7. **Error set union `E1 || E2`** creates a new anonymous error set. Coercion is structural:
   `error{A} → error{A,B}` is implicit; `error{A,B} → error{A}` is a compile error.

8. **Error return traces** — deferred to v2 (optional, debug-mode only).

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

### 1b. HIR Expression: `Question`, `Catch`, `Else`

**File: `crates/lower/src/hir_types/mod.rs`**

Add to the `Expr` enum:
```rust
pub enum Expr {
    // ... existing variants ...
    Question(QuestionExpr),  // ? propagation (both Option and Result)
    Catch(CatchExpr),        // error default
    Else(ElseExpr),          // option default
}

pub struct QuestionExpr {
    pub id: HirId,
    pub expr: Box<Expr>,     // no is_option field — type determined by typecheck
}

pub struct CatchExpr {
    pub expr: Box<Expr>,
    pub fallback: Box<Expr>,
    pub error_binding: Option<Name>,
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

### 2b. Lower `QuestionExpr` (? postfix)

**File: `crates/lower/src/lowering/expr.rs`**

The `?` postfix produces a `QuestionExpr`. No `is_option` field — the typechecker
determines whether it's Option or Result during type inference.

```rust
fn lower_question_expr(e: &ast::QuestionExpr, ctx: &mut LowerCtx) -> Expr {
    let operand = lower_expr(ctx, &e.expr().unwrap())?;
    Expr::Question(QuestionExpr { id: ctx.next_id(), expr: Box::new(operand) })
}
```

The post-typecheck desugar pass uses `TypeMap` to determine match arms:
- `Option<T>` → `Some(v) => v, None => return None`
- `Result<T,E>` → `Ok(v) => v, Err(e) => return Err(e)`

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

`catch` and `else` are desugared to `match` on enums during the pre-typecheck
desugar pass (Phase 5). `?` passes through as `QuestionExpr` and is desugared
post-typecheck using `TypeMap` to determine the correct match arms.

### 4a. Type inference for `?`, `catch`, `else`, `ListLit`

**File: `crates/typecheck/src/typeck/infer.rs`**

These sugar variants now have real inference rules (not `unreachable!()`):

- `QuestionExpr`: infer operand type; if `Option<T>`, result is `T`; if `Result<T,E>`, result is `T`. Error if neither.
- `CatchExpr`: operand must be `Result<T,E>`; fallback must be `T`; result is `T`.
- `ElseExpr`: operand must be `Option<T>`; fallback must be `T`; result is `T`.
- `ListLitExpr`: all elements must be same type `T`; result is `List<T>`.

### 4b. Post-typecheck `?` desugaring

**File: `crates/typecheck/src/typeck/question_desugar.rs`** (new file)

After typecheck completes, `check_with_lang_items` calls `question_desugar::desugar_question`
which walks the HIR and replaces `QuestionExpr` nodes with `Match` nodes using the `TypeMap`
to determine `Some/None` vs `Ok/Err` arms.

### 4c. Error set coercion (Pass 2)

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

### 4b. Typechecking desugared match expressions

Since `try`/`catch`/`else`/`?` are desugared to `match` on `Result<T, E>` or `Option<T>`
before typecheck, no dedicated type rules are needed. The existing `match` typecheck in
`infer_match` handles them:

- `match expr { Ok(v) => v, Err(e) => return Err(e) }` — standard enum match
- `match expr { Some(v) => v, None => fallback }` — standard enum match

### 4c. Error set coercion (Pass 2)

A function `f() -> (E1 || E2)!T` can propagate `E1` errors via `try` — the error set
coerces structurally: `E1`'s values are a subset of `(E1 || E2)`'s values.

Implementation: when checking `return Err(e)` where the function return error set is `S`
and `e` is of error set `E`, verify `E ⊆ S` by checking each variant of `E` exists in `S`.

### 4d. Exhaustiveness on error sets

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

## Phase 5: Desugaring (HIR → HIR)

The desugar pass has two phases:
1. **Pre-typecheck** (in resolver): `catch`, `else`, `ListLit` → `match` (type-independent)
2. **Post-typecheck** (in typecheck): `?` → `match` (type-dependent, uses `TypeMap`)

### 5a. `?` desugaring (post-typecheck)

**File: `crates/typecheck/src/typecheck/question_desugar.rs`**

`expr?` → `match expr { Ok(v) => v, Err(e) => return Err(e) }` (for Result)
or `match expr { Some(v) => v, None => return None }` (for Option).
The type is determined from the `TypeMap` produced by typecheck.

### 5b. `catch` desugaring (pre-typecheck)

**File: `crates/resolver/src/desugar/mod.rs`**

`expr catch fallback` → `match expr { Ok(v) => v, Err(_) => fallback }`

`expr catch |e| handler` → `match expr { Ok(v) => v, Err(e) => handler }`

### 5c. `else` desugaring (pre-typecheck)

`expr else fallback` → `match expr { Some(v) => v, None => fallback }`

### Phase 5 Tests

| Test | What it proves |
|------|---------------|
| `try_desugars_to_match` | After desugaring, HIR contains Match, not Try |
| `catch_desugars_to_match` | After desugaring, HIR contains Match, not Catch |
| `else_desugars_to_match` | After desugaring, HIR contains Match, not Else |
| `option_question_desugars` | `expr?` → match with `None => return None` |
| `else_desugars_to_match` | After desugaring, HIR contains Match, not Else |

---

## Phase 6: IR Lowering + VM Execution

Since `?`/`catch`/`else` desugar to `match` on `Result<T, E>` or `Option<T>` (user-defined
generic enums), the IR and VM need **zero new ops** for error handling. The existing
`Match` terminator and `EnumNew`/`VariantPayload` instructions handle everything:

```
open(path)?
// desugars to (after typecheck):
match open(path) {
    Ok(v) => v,
    Err(e) => return Err(e),
}
// lowers to (simplified):
//   %0 = Call open(path)
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

### 7c. `?` is the sole propagation node

The parser produces `QuestionExpr` for `?` postfix. The old `TryExpr` CST node
(which conflated `try expr` and `expr?`) has been removed. `try` is no longer
a keyword — it's a valid identifier.

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
| Unify `try` (Result) and `?` (Option) | **✅ Resolved** — `?` is now universal; see `error-handling-redesign.md` |
| `Result` as builtin vs stdlib type | Currently stdlib — revisit if perf requires builtin |
| Error set with data payloads | Zig chose unit-only; Axiom follows. Revisit if evidence shows need |
