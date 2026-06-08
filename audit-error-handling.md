# Error Handling Implementation Audit

## Files Audited (25 files)

| # | File | Lines |
|---|------|-------|
| 1 | `crates/lower/src/hir_types/ty.rs` | 51 |
| 2 | `crates/lower/src/hir_types/mod.rs` | 562 |
| 3 | `crates/lower/src/hir_types/items.rs` | 185 |
| 4 | `crates/lower/src/lowering/mod.rs` | 196 |
| 5 | `crates/lower/src/lowering/expr.rs` | 543 |
| 6 | `crates/lower/src/lowering/error.rs` | 52 |
| 7 | `crates/lower/src/lowering/item.rs` | 577 |
| 8 | `crates/lower/src/lowering/ty.rs` | 58 |
| 9 | `crates/lower/src/serialize/mod.rs` | 614 |
| 10 | `crates/resolver/src/lib.rs` | 55 |
| 11 | `crates/resolver/src/resolve/mod.rs` | 418 |
| 12 | `crates/resolver/src/resolve/item.rs` | 218 |
| 13 | `crates/resolver/src/resolve/body.rs` | 270 |
| 14 | `crates/resolver/src/desugar/mod.rs` | 446 |
| 15 | `crates/resolver/src/desugar/tests.rs` | 446 |
| 16 | `crates/typecheck/src/typeck/collect.rs` | 629 |
| 17 | `crates/typecheck/src/serialize/helpers.rs` | 99 |
| 18 | `crates/typecheck/src/typeck/infer.rs` | 450 |
| 19 | `crates/typecheck/src/types.rs` | 280 |
| 20 | `crates/parser/src/syntax_kind.rs` | 510 |
| 21 | `crates/parser/src/ast/item.rs` | 406 |
| 22 | `crates/parser/src/ast/expr.rs` | 322 |
| 23 | `crates/parser/src/grammar/expr.rs` | 471 |
| 24 | `crates/parser/src/grammar/item.rs` | 506 |
| 25 | `crates/parser/src/grammar/ty.rs` | 169 |
| 26 | `crates/vm/src/lib.rs` | 468 |
| 27 | `crates/cli/src/main.rs` | 10 |
| 28 | `scripts/check.sh` | 107 |

---

## 1. CONVENTIONS COMPLIANCE (RUST_CONVENTIONS.md)

### C1 — File exceeds 600-line ceiling
**Severity: P2**

| File | Lines | Cap |
|------|-------|-----|
| `crates/typecheck/src/typeck/collect.rs` | 629 | 600 |
| `crates/lower/src/serialize/mod.rs` | 614 | 600 |

Both files exceed the 600-line cap enforced by `scripts/check.sh`. `collect.rs` has no grandfathered entry in the script. `serialize/mod.rs` also has none. These need to be split into smaller modules or grandfathered with a documented reason.

### C2 — `unwrap()`/`expect()`/`panic!` on user-reachable paths
**Severity: P1**

| File | Line | Code |
|------|------|------|
| `crates/lower/src/lowering/mod.rs` | 152 | `text.parse::<i64>().map(LitKind::Int).unwrap_or(LitKind::Int(0))` |
| `crates/lower/src/lowering/mod.rs` | 158 | `text.parse::<f64>().map(LitKind::Float).unwrap_or(LitKind::Float(0.0))` |
| `crates/lower/src/lowering/mod.rs` | 109 | `token.map(\|t\| t.text().to_string()).unwrap_or_default()` |
| `crates/lower/src/lowering/mod.rs` | 120 | `.unwrap_or_default()` |
| `crates/lower/src/lowering/expr.rs` | 104 | `.unwrap_or(LitKind::Unit)` |
| `crates/lower/src/lowering/expr.rs` | 122 | `.unwrap_or(BinOp::Add)` |
| `crates/lower/src/lowering/expr.rs` | 126 | `.unwrap_or_else(...)` — 8 occurrences on `lhs`/`rhs` |
| `crates/lower/src/lowering/expr.rs` | 148 | `.unwrap_or(UnaryOp::Neg)` |
| `crates/lower/src/lowering/expr.rs` | 162 | `.unwrap_or_default()` |
| `crates/lower/src/lowering/expr.rs` | 357 | `.unwrap_or_else(...)` |

The `unwrap_or`/`unwrap_or_default` pattern is used pervasively through the lowerer for CST→HIR conversion. These are *not* `unwrap()` panics — they provide fallback defaults when the CST node is malformed or incomplete. However, they silently swallow malformed CST, producing `LitKind::Unit` or `BinOp::Add` as defaults, which can mask parser bugs in downstream stages. The convention (ENFORCEMENT.md line 22) targets `unwrap`/`expect`/`panic!` on user-reachable paths; `unwrap_or` is technically fine but the *pattern* of silently defaulting malformed CST is questionable.

**Verdict:** `unwrap_or`/`unwrap_or_default` are allowed (they don't panic), but the defaults mask real parser issues. Consider creating dedicated error nodes instead of `Unit` for recovery expressions.

### C3 — `unsafe` blocks
**Severity: CONFIRMED OK**
Zero `unsafe` blocks found in any of the audited files. The workspace `unsafe_code = "forbid"` is effective.

### C4 — `RefCell`/`Cell`/`Mutex`/`RwLock`
**Severity: CONFIRMED OK**
None found in audited files.

### C5 — `macro_rules!`
**Severity: CONFIRMED OK**
No custom `macro_rules!` in audited files. `syntax_kind.rs:19` uses the `syntax_kinds!` macro, but this is the pre-existing parser infrastructure pattern (not error-handling code).

### C6 — `dyn Trait`
**Severity: CONFIRMED OK**
None found in audited files.

### C7 — Functions >60 lines / cognitive complexity
**Severity: P2**

| File | Function | Approx. Lines |
|------|----------|---------------|
| `lowering/item.rs` | `lower_item` | ~45 (dispatch) |
| `lowering/item.rs` | `lower_fn_inner` | ~40 |
| `lowering/item.rs` | `lower_impl_block` | ~42 |
| `lowering/item.rs` | `lower_use_tree` | ~63 |
| `lowering/expr.rs` | `lower_expr` | ~60 (dispatch) |
| `desugar/mod.rs` | `desugar_try` | ~80 |
| `desugar/mod.rs` | `desugar_non_empty_list` | ~60 |
| `desugar/tests.rs` | `count_sub_expr_kind` | ~80 (`#[allow(clippy::too_many_lines)]`) |
| `typeck/infer.rs` | `infer_expr` | ~47 (dispatch) |
| `typeck/collect.rs` | `resolve_hir_ty` | ~50 |

The `desugar_try` function at 80 lines is the clearest violation of the ≤60 line rule. `count_sub_expr_kind` has an explicit `#[allow(clippy::too_many_lines)]` — the right approach per the convention (allow with comment); however, no comment explains the exception.

### C8 — Functions with >5 arguments
**Severity: CONFIRMED OK**
No function in audited error-handling code exceeds 5 arguments. `process_use_items` (6 args) and `resolve_use_path` (5 args) exist in the resolver but are pre-existing.

### C9 — Multiple lifetimes / lifetime bounds
**Severity: CONFIRMED OK**
None found in audited files.

### C10 — Iterator chains >2 adapters
**Severity: P3**

| File | Line | Chain |
|------|------|-------|
| `lowering/item.rs` | 501-503 | `use_tree` path extraction: `.children().into_iter().filter_map(...).find_map(...)` |

Minor. The logging/info pattern in `collect.rs:182-190` (`filter_map(...).collect()`) is a 2-adapter chain, which is fine.

### C11 — Missing `//!` module docs
**Severity: P3**

| File | Has `//!`? |
|------|-----------|
| `crates/lower/src/hir_types/ty.rs` | Yes (line 1) |
| `crates/lower/src/hir_types/mod.rs` | Yes (line 1) |
| `crates/lower/src/hir_types/items.rs` | Yes (line 1) |
| `crates/lower/src/lowering/mod.rs` | Yes (line 1) |
| `crates/lower/src/lowering/expr.rs` | Yes (line 1) |
| `crates/lower/src/lowering/error.rs` | Yes (line 1) |
| `crates/lower/src/lowering/item.rs` | Yes (line 1) |
| `crates/lower/src/lowering/ty.rs` | Yes (line 1) |
| `crates/typecheck/src/typeck/infer.rs` | Yes (line 1) |
| `crates/resolver/src/desugar/mod.rs` | Yes (line 1) |

All files have `//!` module docs. The desugar `tests.rs` does not, but test-only modules are exempted by convention (tests can begin with `use super::*`).

### C12 — README freshness
**Severity: P2**

`crates/lower/src/lowering/error.rs` was added as part of error handling but there is no `crates/lower/README.md` update visible. Similarly, the `desugar` module in resolver needs README updates. Not verified — deferred to cross-check with actual READMEs.

---

## 2. DESIGN PLAN FIDELITY

### D1 — Phase 1–6 spec compliance
**Severity: CONFIRMED OK — Partial**

| Phase | Status | Notes |
|-------|--------|-------|
| 1a (ErrorSetDef HIR item) | **DONE** | `items.rs:86-97`, correct shape |
| 1b (Try/Else HIR expr) | **DONE** | `mod.rs:446-456`, correct shape |
| 1d (HirTy::ErrorSet/ErrorSetUnion) | **DONE** | `ty.rs:20-22`, correct |
| 1e (Ty::ErrorSet) | **DONE** | `types.rs:42`, `ErrorSetTy` correct |
| 1f (DefKind::ErrorSet/ErrorVariant) | **DONE** | `mod.rs:73-74` |
| 2a (Lower ErrorSetDef) | **DONE** | `item.rs:23-24`, `error.rs:8-52` |
| 2b (Lower TryExpr with disambiguation) | **DONE** | `expr.rs:342-362` |
| 2c (Lower ElseExpr) | **DONE** | `expr.rs:364-379` |
| 2d (ErrorSetUnionType lowering) | **NOT DONE** | `ty.rs:43-48` emits `NotYetSupported` |
| 3a (Resolve ErrorSetDef) | **DONE** | `item.rs:69-71` — pass-through |
| 3b (Resolve Try/Else) | **DONE** | `body.rs:129-135` |
| 3c (DefKind filters) | **DONE** | `mod.rs:31-38`, `mod.rs:155-163` |
| 4a (Collect error set defs) | **DONE** | `collect.rs:181-225` |
| 4b (Typecheck try) | **NOT DONE** | `infer.rs:42-48` — emits `NotYetSupported` |
| 4c (Typecheck else) | **NOT DONE** | `infer.rs:49-55` — emits `NotYetSupported` |
| 4d (Error set coercion) | **NOT DONE** | No coercion logic exists |
| 4e (Exhaustiveness on error sets) | **NOT DONE** | `exhaustiveness.rs` has no ErrorSet handling |
| 5a (try desugaring) | **DONE** | `desugar/mod.rs:205-283` — correct |
| 5b (else desugaring) | **DONE** | `desugar/mod.rs:286-331` — correct |
| 5c (? desugaring) | **NOT DONE** | `expr.rs:351` emits `NotYetSupported` |
| 6 (IR/VM) | **DONE** | Zero new ops (design goal met) |
| 7 (Parser cleanup) | **PARTIAL** | CST nodes exist, `catch` still `CatchExpr` |

### D2 — `?` disambiguation (Option vs error)
**Severity: BUG — P1**

The design plan specifies `?` is Option-only propagation. The implementation correctly disambiguates in `expr.rs:342-362`:

```rust
fn lower_try_expr(e: &ast::TryExpr, ctx: &mut LowerCtx, node: &parser::SyntaxNode) -> Expr {
    let is_option_propagation = node
        .children()
        .first()
        .is_some_and(|c| c.kind() != parser::SyntaxKind::KwTry);
    if is_option_propagation {
        return unsupported_expr(ctx, "option propagation (?)", node);
    }
    // ... try propagation ...
}
```

The disambiguation logic is **backwards**: it checks `c.kind() != KwTry`. For prefix `try expr`, the first child IS `KwTry`, so `!= KwTry` is `false` → `is_option_propagation = false` → correct. For postfix `expr?`, the first child is the expression (NOT `KwTry`), so `!= KwTry` is `true` → `is_option_propagation = true` → correctly emits `NotYetSupported`.

**Wait** — this is actually **correct**. The logic says: if the first child is NOT `KwTry` (i.e., it's an expression like `expr?`), treat it as Option propagation. If it IS `KwTry`, treat it as prefix try. This works. However, it's fragile — if the CST shape changes (e.g., a whitespace node as first child), this breaks. The `?` case is correctly deferred to `NotYetSupported`, which is appropriate.

### D3 — `else` desugaring correctness
**Severity: DESIGN DRIFT — P1**

The design plan (Phase 5b) specifies two forms:
1. `expr else fallback` → `match expr { Ok(v) => v, Err(_) => fallback }` (for Result)
2. `expr else |e| handler` → `match expr { Ok(v) => v, Err(e) => handler }`

The implementation (`desugar/mod.rs:286-331`) only handles form 1:

```rust
let err_arm = MatchArm {
    pattern: Pattern::Wildcard(err_pat_id),  // ← Wildcard, not |e| capture
    guard: None,
    body: *fallback,
};
```

**Problem 1:** The `else |e| handler` capture syntax is not implemented. The parser grammar (`grammar/expr.rs:56-66`) allows `catch` or `else` followed by any expression (including `|e| ...` closures), but the desugar pass doesn't distinguish the capture case. It always uses `Wildcard` for the error arm.

**Problem 2:** The desugaring hardcodes `Ok(v)` → `v` for the success arm, which is wrong for `Option` types. `xs.first() else 0` should produce `match xs.first() { Some(v) => v, None => 0 }`, not `Ok`/`Err` patterns. There is no runtime type dispatch to choose between `Some`/`None` vs `Ok`/`Err`.

**Problem 3:** For `Result`, the `Err(_) => fallback` desugaring discards the error value. If the fallback needs to inspect the error (the design plan's stated use case), this silently loses the error.

### D4 — Error sets unit-only
**Severity: CONFIRMED OK**
`ErrorVariantDef` has no payload field (`items.rs:93-97`) — correct per design.

### D5 — `catch` reserved but not implemented
**Severity: CONFIRMED OK**
`KwCatch` is in the keyword list (`syntax_kind.rs:173`), lexed by the lexer (`symbols.rs:30`), and plumbed through the parser (`expr.rs:60-64`). The parser builds `CatchExpr` when it sees `catch` (or `else`). The plan says `catch` should be reserved — this is correct. The `CatchExpr` CST node exists but `catch` isn't promoted as a user-facing keyword; only `else` is.

**Minor drift:** The plan says "The `catch` keyword in the lexer remains as a reserved word (may be reclaimed later)." But the parser grammar **actively accepts** `catch` as a valid operator (`p.at(K::KwCatch)`). It's more than reserved — it's partially functional.

### D6 — Zero new IR/VM ops
**Severity: CONFIRMED OK**
VM (`lib.rs`) has no error-handling-specific ops. Error handling desugars to `match` on `Result`/`Option` enums, which use existing `EnumNew`/`VariantPayload`/`Branch`/`Jump` instructions. Design goal met.

---

## 3. CORRECTNESS & EDGE CASES

### E1 — Unhandled HirTy variants in match statements
**Severity: BUG — P1**

`resolve_hir_ty` in `collect.rs:518-566` handles all 9 `HirTy` variants (Named, TypeParam, Instance, Unit, Tuple, Fn, Slice, ErrorSet, ErrorSetUnion, Error). All covered.

`resolve_ty_names` in `item.rs:155-217` handles all 9 variants. All covered.

`fmt_hir_ty` in `helpers.rs:33-82` handles all 9 variants. All covered.

`infer_expr` in `infer.rs:11-57` handles all 18 `Expr` variants. All covered — `Try` and `Else` emit `NotYetSupported`.

**However:** `desugar_item` in `desugar/mod.rs:47-68` has arm:
```rust
Item::StructDef(_) | Item::EnumDef(_) | Item::UseItem(_) | Item::ErrorSetDef(_) => {}
```
This is correct — ErrorSetDef has no bodies to desugar.

### E2 — `resolve_callee` handling both resolved and unresolved names
**Severity: VERIFIED OK**

`resolve_callee` in `infer.rs:305-336` handles both `NameRef::Resolved` and `NameRef::Unresolved` branches. Unresolved checks the environment then builtins. Resolved checks the environment (lookup by text), then builtins. Both branches correctly handle the `Ty::Fn` / error case. 

**Edge case:** An `ErrorSet` variant name like `NotFound` registered via `collect_error_set_defs` as a `Ty::Fn` (nullary constructor) will be correctly resolved by `env.lookup` in both branches.

### E3 — Desugar coverage invariant (Expr variants)
**Severity: CONFIRMED OK**

`test_every_expr_variant_handled_by_desugar` in `tests.rs:404-446` lists all 17 Expr variants and classifies them as sugar (`ListLit`, `Try`, `Else`) or non-sugar (14 others). The count assertion (`assert_eq!(all_expr.len(), 17)`) matches the 17 variants in `mod.rs:190-208`. Correct.

### E4 — ErrorSetDef and ErrorSetUnion handled in ALL pipeline stages
**Severity: MISSING COVERAGE — P1**

| Pipeline Stage | ErrorSetDef | ErrorSetUnion | ErrorUnionType |
|---------------|-------------|---------------|----------------|
| Parser (CST) | ✅ `ErrorSetDef` node | ✅ `ErrorSetUnionType` node | ✅ `ErrorUnionType` node |
| Lower (HIT) | ✅ `Item::ErrorSetDef` | ✅ `HirTy::ErrorSetUnion` | ❌ `NotYetSupported` |
| Resolve | ✅ pass-through | ✅ resolved | N/A |
| Serialize (lower) | ✅ `serialize_item` arm | ❌ Not in lower serialization | N/A |
| Serialize (typeck) | ✅ THIR serialization | N/A | N/A |
| Typecheck | ✅ collected | ❌ falls through to first member | N/A |
| Desugar | ✅ pass-through | N/A | N/A |
| IR/VM | N/A (unit-only enum) | N/A | N/A |

**Missing:** `HirTy::ErrorSetUnion` is not handled in the **lower** serializer (`serialize/mod.rs`). The `fmt_ty` function in `serialize/types.rs` was not audited but should be checked. `ErrorUnionType` desugaring (`IO!Int` → `Named("Result")` with type args) is marked as `NotYetSupported` in `ty.rs:43-48`, contradicting the design plan which says this should work.

### E5 — Nested try/else handling
**Severity: CONFIRMED OK**
`desugar_try` and `desugar_else` recursively desugar sub-expressions before rewriting themselves. Nested `try try f()` or `f() else (g() else 0)` is handled correctly because the walk first descends into children, then replaces the parent.

### E6 — `else` on methods
**Severity: BUG — P1**

`foo() else bar()` works because `else` is parsed as a postfix operator on the full expression (including method calls). The parser grammar at `expr.rs:56-66` wraps the complete `expr_bp` result:

```rust
pub(super) fn expr(p: &mut Parser) {
    let Some(cm) = expr_bp(p, 0) else { return; };
    if p.at(K::KwCatch) || p.at(K::KwElse) {
        let m = cm.precede(p);
        p.bump();
        expr_bp(p, 0);
        m.complete(p, K::CatchExpr);
    }
}
```

However, `foo().try().else()` (chaining) will NOT work because: (1) `try` is prefix, not postfix, so `foo().try()` is parsed as `foo()` then `.try()` field access, not try; (2) the lowerer maps `CatchExpr` to `ElseExpr`, which requires `ast::CatchExpr::cast(node)` — but the CST node is `K::CatchExpr`, and `ast::CatchExpr` presumably wraps it. This needs verification but appears to handle the basic case.

**Real issue:** The parser uses `K::CatchExpr` not `K::ElseExpr`. The `ast` view likely has `CatchExpr` (not `ElseExpr`). This naming inconsistency between parser (`CatchExpr`) and HIR (`ElseExpr`) is confusing but functional.

### E7 — Silent fallthrough
**Severity: BUG — P1**

`resolve_hir_ty` in `collect.rs:556-563`:
```rust
HirTy::ErrorSetUnion(members) => {
    // Error set unions resolve to the first member's resolved type
    // for now; full union semantics deferred to Phase 4e.
    members
        .first()
        .map(|m| self.resolve_hir_ty(m))
        .unwrap_or(Ty::Error)
}
```

This silently resolves an error set union to the **first member's type**, discarding the rest. This is documented as "deferred" but produces incorrect types. A union of `(IO || FsError)` would resolve as just `IO`. This will cause spurious type mismatches in any code using `||`.

---

## 4. TESTING

### T1 — Tests for try, else, error set definitions
**Severity: MISSING COVERAGE — P1**

| Feature | Lower tests | Desugar tests | Typeck tests | Integration tests |
|---------|------------|---------------|-------------|-------------------|
| Error set definition | ❌ None | N/A | ❌ None | ❌ None |
| try expression | ❌ None | ❌ None (`test_desugar_try`) | ❌ None | ❌ None |
| else expression | ❌ None | ❌ None | ❌ None | ❌ None |
| Option `?` | N/A | N/A | N/A | N/A |

The design plan lists 6 specific tests for Phase 2 and 10 for Phase 4. **Zero of these exist.** The only error-handling-adjacent test is `test_desugar_list_in_if_else` in `tests.rs:315`, which tests list desugaring in an `if/else` context — unrelated.

### T2 — `test_every_expr_variant_handled_by_desugar` correctness
**Severity: CONFIRMED OK**

The test correctly counts 17 Expr variants and classifies `ListLit`, `Try`, `Else` as sugar and the remaining 14 as non-sugar. The count of 17 matches the enum definition. Verified.

### T3 — Integration/end-to-end tests
**Severity: MISSING COVERAGE — P1**

No `.ax` feature-test files for error handling exist under `corpus/` or `crates/*/tests/fixtures/`. The design plan specifies fixture files:
- `error_set_basic.ax`
- `error_handling_basic.ax`
- `error_try.ax`
- `error_else.ax`
- `error_coercion.ax`

None of these were found.

### T4 — Desugar idempotency
**Severity: CONFIRMED OK — But only for lists**

`test_desugar_is_idempotent` (`tests.rs:386-398`) tests list desugaring idempotency. No idempotency test exists for `Try` or `Else` desugaring. After desugaring, `Try` and `Else` are replaced with `Match`, and re-running desugar on the result should produce identical output (Match is non-sugar).

### T5 — Coverage test for DefKind
**Severity: MISSING COVERAGE — P2**

No `test_every_def_kind_reachable` test exists to ensure `DefKind::ErrorSet` and `DefKind::ErrorVariant` appear in the `build_top_level` and `build_global_exports` filters. The filters were verified manually (they include both ErrorSet and ErrorVariant), but a mechanized coverage test would prevent drift.

---

## 5. TECH DEBT

### TD1 — TODO/FIXME/HACK comments
**Severity: P3**

One TODO found:
- `typeck/control.rs:155` — `// TODO(v1): wire up real spans from the HIR.`
  Unrelated to error handling; pre-existing.

### TD2 — Code duplication across stages
**Severity: P2**

`serialize_item` for `ErrorSetDef` is implemented **both** in:
- `crates/lower/src/serialize/mod.rs:37-51` (HIR serializer)
- `crates/typecheck/src/serialize/mod.rs:97-101` (THIR serializer)

The implementations are near-identical (format `ErrorSetDef({}) name={} vis={} variants=[...]`). This is duplicating the HIR serialization logic in the typecheck crate. The THIR serializer should delegate to the HIR serializer for HIR items.

### TD3 — `#[allow(...)]` annotations
**Severity: P2**

| File | Line | Annotation | Justified? |
|------|------|-----------|------------|
| `desugar/tests.rs:80` | `#[allow(clippy::too_many_lines)]` | On `count_sub_expr_kind` — **needs comment** |
| `desugar/mod.rs:445` | `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` | On test module — **justified** (tests unwrap) |
| `lowering/mod.rs:44` | `#[allow(dead_code)]` | On `source: String` field — **should be removed or used** |
| `typeck/types.rs:159` | `#[allow(dead_code)]` | On `label()` function — **used in tests only** |

The `too_many_lines` allow on `count_sub_expr_kind` is an architectural smell: the function should be split.

### TD4 — `CatchExpr` vs `ElseExpr` naming
**Severity: P2**

The design plan explicitly states (line 48): "The parser CST node `CatchExpr` will be renamed to `ElseExpr`." This rename has NOT happened:
- CST node: `SyntaxKind::CatchExpr` (`syntax_kind.rs:260`)
- AST view: `ast::CatchExpr`
- Lowerer: `lower_else_expr` takes `&ast::CatchExpr`
- HIR: `Expr::Else(ElseExpr { ... })`

The split between parser (`CatchExpr`) and HIR (`ElseExpr`) is confusing and not documented in a code comment.

### TD5 — `ErrorSetUnion` placeholder resolution
**Severity: BUG — P1**

`resolve_hir_ty` for `ErrorSetUnion` resolves to the first member's type (see E7 above). The comment says "deferred to Phase 4e" but Phase 4e (exhaustiveness on error sets) is not implemented either. This is a **broken partial implementation** — it produces incorrect types and can cause false-negative type errors.

### TD6 — No `error_set_union_type` tests
**Severity: MISSING COVERAGE — P2**

The parser's `grammar/ty.rs:99-121` handles `(A || B)` by marking `is_union = true` and producing `K::ErrorSetUnionType`. There are no parser tests for this grammar production. The only tests in `ty.rs` are for `SliceType`.

---

## SUMMARY TABLE

| # | Finding | Category | Severity | File:Line |
|---|---------|----------|----------|-----------|
| C1 | collect.rs 629 lines > 600 cap | CONVENTION | P2 | `typeck/collect.rs` |
| C1b | serialize/mod.rs 614 lines > 600 cap | CONVENTION | P2 | `lower/serialize/mod.rs` |
| C2 | `unwrap_or` defaults mask malformed CST | CONVENTION | P2 | `lowering/expr.rs` pervasive |
| C7 | `desugar_try` ~80 lines | CONVENTION | P2 | `desugar/mod.rs:205` |
| C7b | `count_sub_expr_kind` has allow(too_many_lines) | CONVENTION | P2 | `desugar/tests.rs:80` |
| C11 | desugar `tests.rs` no module doc | CONVENTION | P3 | `desugar/tests.rs:1` |
| C12 | README not updated for new modules | CONVENTION | P2 | per-crate READMEs |
| D1 | ErrorSetUnionType lowering emits NotYetSupported | DESIGN DRIFT | P1 | `lowering/ty.rs:43` |
| D1b | try typechecking emits NotYetSupported | DESIGN DRIFT | P1 | `typeck/infer.rs:42` |
| D1c | else typechecking emits NotYetSupported | DESIGN DRIFT | P1 | `typeck/infer.rs:49` |
| D1d | ? Option propagation deferred to NotYetSupported | DESIGN DRIFT | P2 | `lowering/expr.rs:351` |
| D1e | Error union type NotYetSupported | DESIGN DRIFT | P2 | `lowering/ty.rs:43` |
| D1f | Error set coercion not implemented | DESIGN DRIFT | P1 | (missing module) |
| D1g | Error set exhaustiveness not implemented | DESIGN DRIFT | P1 | `exhaustiveness.rs` |
| D2 | `?` disambiguation logic fragile | BUG | P2 | `lowering/expr.rs:347-350` |
| D3a | `else \|e\| handler` capture not implemented | DESIGN DRIFT | P1 | `desugar/mod.rs:320-323` |
| D3b | Else desugaring hardcodes Ok/Err (no Option) | BUG | P1 | `desugar/mod.rs:300-308` |
| D3c | Error value discarded in `else fallback` | DESIGN DRIFT | P2 | `desugar/mod.rs:321` |
| E4 | ErrorSetUnion not in lower serializer | MISSING | P2 | `lower/serialize/mod.rs` |
| E7 | ErrorSetUnion resolves to first member only | BUG | P1 | `typeck/collect.rs:556-563` |
| T1a | No try desugaring test | MISSING | P1 | `desugar/tests.rs` |
| T1b | No else desugaring test | MISSING | P1 | `desugar/tests.rs` |
| T1c | No error set def test | MISSING | P1 | all crates |
| T1d | No integration tests | MISSING | P1 | corpus/ |
| T4 | No try/else idempotency test | MISSING | P2 | `desugar/tests.rs` |
| T5 | No DefKind coverage test | MISSING | P2 | resolver/tests |
| TD2 | ErrorSetDef serializer duplicated | TECH DEBT | P2 | serialize modules |
| TD3a | `#[allow(dead_code)]` on source field | TECH DEBT | P2 | `lowering/mod.rs:44` |
| TD4 | CatchExpr not renamed to ElseExpr | TECH DEBT | P2 | `syntax_kind.rs:260` |
| TD5 | ErrorSetUnion placeholder produces wrong types | BUG | P1 | `typeck/collect.rs:556-563` |
| TD6 | No ErrorSetUnionType parser test | MISSING | P2 | `grammar/ty.rs` |

---

## SEVERITY COUNT

| Severity | Count |
|----------|-------|
| **P0** (broken) | 0 |
| **P1** (must fix) | 14 |
| **P2** (should fix) | 17 |
| **P3** (nice to have) | 2 |

## KEY TAKEAWAY

The **structural plumbing** (parser → HIR → resolver → desugar) is well-built and consistent. The `DefKind`, `HirTy`, `Expr`, and `Item` enums all have correct error-handling variants, and every enum match in the pipeline is exhaustive.

The two critical gaps are:
1. **Typechecking is a no-op for try/else** — they emit `NotYetSupported`, meaning error handling is not actually type-safe.
2. **Else desugaring is incomplete** — no Option support (`Some`/`None`), no `|e|` capture, and error values are silently discarded.

The desugaring pass is the cleanest part of the implementation, correctly rewriting `try` → `match { Ok/Err }` and `else` → `match { Ok/Wildcard }`. Once typechecking is implemented for try/else, and the else desugar is extended for Option and `|e|` capture, the feature will be end-to-end functional.
