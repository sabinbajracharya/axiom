# HIR Desugar Pass — Implementation Plan

> **Status: Approved for implementation (June 2026).** This doc plans the work
> described in `lang-items-and-desugaring-design.md` §4 ("the desugaring stage"),
> accelerated to *now* rather than waiting for the 2nd/3rd sugar trigger.

## 0. Why now

The trigger rule (§4 of the lang-items doc) was: wait until a 2nd or 3rd sugar
form lands, because moving one sugar into a new HIR phase is a pure refactor that
buys nothing alone. That reasoning was sound *when this was just the list-literal
cleanup*. But there are now concrete reasons to land the pass first:

- `loop x in` is scaffolded (parsed, in HIR, typeck emits `NotYetSupported`) —
  wiring it means adding yet another ad-hoc lowering. Better to land the desugar
  framework first so `loop x in` → `Iterator` goes through the same pass.
- The lang-item infrastructure (§3.2/§3.3) is proven and stable — the
  prerequisite that the HIR pass needs is already in place.
- Doing it now is a bounded, testable, behaviour-preserving change: list literals
  work the same, just via a different pipeline location.

**The promise:** after this pass lands, adding new sugar (map literals, range,
`loop x in`, compound assign) is one desugar rule + one lang item each — no new
ad-hoc lowering, no new typeck special-cases.

## 1. What changes (before/after)

### Before (current)

```
parse → lower HIR → resolve names → resolve lang items
                                         ↓
                         typeck (infer_list_lit special-case)
                                         ↓
                         IR lower (lower_list_lit — string-keyed calls)
                                         ↓
                         VM
```

- `[a, b, c]` stays as `Expr::ListLit` through HIR and typeck.
- Typeck has `infer_list_lit`: manually types the literal as `List<T>`,
  unifies element types from elements, stamps the type with the resolved
  lang-item `DefId`.
- IR `lower_list_lit`: synthesizes `"List::with_capacity"` / `"List::push"` /
  `"List::new"` call chains via string-keyed dispatch.

### After (target)

```
parse → lower HIR → resolve names → resolve lang items
                                         ↓
                         ** desugar pass (ListLit → Block+Call+MethodCall) **
                                         ↓
                         typeck (no infer_list_lit — types calls normally)
                                         ↓
                         IR lower (no lower_list_lit — sees normal calls)
                                         ↓
                         VM
```

- `[a, b, c]` is rewritten into a `Block` expression containing plain HIR
  `Call` + `MethodCall` + `VarStmt` + `ExprStmt` nodes, all using resolved
  lang-item `DefId`s.
- Typeck sees normal function/method calls. `push`'s `inout self, sink element: T`
  signature types element unification naturally — no special-case needed.
- IR sees normal calls. No `lower_list_lit`. No string-keyed dispatch for list
  literals.
- `Expr::ListLit` is **removed** from the HIR enum entirely (it no longer
  survives past the desugar pass).

## 2. The desugar pass — detailed design

### 2.1 Location

A new module `axiom-hir/src/desugar.rs`. Public entry point:

```rust
/// Rewrite every sugar expression in `hir` into its core form, using resolved
/// lang-item IDs. After this pass, no `Expr::ListLit` remains in the tree.
pub fn desugar(hir: &mut Hir, lang_items: &LangItems) -> DesugarResult;
```

`DesugarResult` carries the new highest `HirId` allocated (so the caller knows
the ID range) plus any desugar-specific diagnostics.

### 2.2 Algorithm

Walk every function body in every `Item` in `hir.items`:

1. **For each `FnDef`:** walk its body block.
2. **For each `Block`:** visit each `Stmt`, then the optional `tail`.
3. **When a `Stmt` or `tail` contains `Expr::ListLit`:**
   replace the list literal with the desugared form (see §2.3).
4. **Continue recursively** into nested blocks (if/match/loop bodies).

This is a **bottom-up** transform: desugar inner expressions first (so nested
`[[a], [b, c]]` works), then the outer.

### 2.3 Desugaring `[e0, e1, ..., eN]` (non-empty)

The HIR before:

```
ListLitExpr { id: N, elements: [e0, e1, e2] }
```

The HIR after (a `Block` expression):

```
Block {
    id: <fresh>,
    stmts: [
        VarStmt {
            id: <fresh>,
            pattern: Pattern::Binding("__list_0"),
            ty: None,
            value: Call {
                id: <fresh>,
                callee: NameRef::resolved(list_with_capacity_def_id, "with_capacity"),
                qualifier: None,
                args: [
                    Lit(LitExpr { id: <fresh>, kind: LitKind::Int(3) }),
                ],
            },
        },
        ExprStmt {
            id: <fresh>,
            expr: MethodCall {
                id: <fresh>,
                receiver: Path(PathExpr {
                    id: <fresh>,
                    name_ref: NameRef::resolved(var_stmt_id, "__list_0"),
                }),
                method: "push",
                args: [e0],
            },
        },
        // ... one ExprStmt per remaining element
    ],
    tail: Some(Box::new(Path(PathExpr {
        id: <fresh>,
        name_ref: NameRef::resolved(var_stmt_id, "__list_0"),
    }))),
}
```

Key design decisions:

- **`VarStmt` (mutable)** not `ValStmt` — because `push` takes `inout self`,
  typeck requires a mutable binding.
- **Fresh names** — `__list_0`, `__list_1`, … using a monotonically
  increasing counter so generated names don't collide with user names (the `__`
  prefix is invalid in Axiom source, guaranteed by the lexer).
- **Fresh `HirId`s** — every generated node gets a new ID from a counter
  starting at `max_existing_id + 1`.
- **`NameRef::resolved` for the temp** — the `VarStmt`'s `pattern` binds an
  identifier; all `Path` references to it point at the `VarStmt.id` as their
  `DefId`. This means the desugared HIR is fully name-resolved (no lazy
  re-resolution needed).
- **`Call::qualifier` is `None`** — the lang-item function is referenced by
  `DefId` via `NameRef::resolved`, no string path needed. The `callee` text
  field is the function name (for diagnostics).
- **`MethodCall::method` is `"push"`** — it stays a plain string (not a lang
  item) because `push` is not a special compiler method — it's just the method
  the desugar calls. Typeck resolves it against the receiver type normally.

### 2.4 Desugaring `[]` (empty)

The HIR before:

```
ListLitExpr { id: N, elements: [] }
```

The HIR after:

```
Call {
    id: <fresh>,
    callee: NameRef::resolved(list_new_def_id, "new"),
    qualifier: None,
    args: [],
}
```

No temp variable needed — it's a single call. The element type is inferred from
the surrounding context (type annotation or usage). This replaces the current
`try_annotated_empty_list` special-case in typeck.

### 2.5 Fresh ID generation

The pass takes a `next_id: &mut usize` parameter, seeded to `max_hir_id + 1`
before the pass begins. Every generated node calls `next_id()` / `fresh_id()`:

```rust
fn fresh_id(next: &mut usize) -> HirId {
    let id = HirId(*next);
    *next += 1;
    id
}
```

After the pass, the caller records the final `next_id` value. No downstream
stage depends on IDs being contiguous or sorted — they only need to be unique.

### 2.6 Temp variable naming

```rust
fn fresh_temp_name(counter: &mut usize) -> String {
    let name = format!("__list_{counter}");
    *counter += 1;
    name
}
```

The `__` prefix is guaranteed safe because the Axiom lexer rejects
double-underscore identifiers in user source. Generated names distinguish
desugared temps from user variables in diagnostics and dumps.

### 2.7 What is NOT desugared (yet)

- `Expr::Assign` for indexed assignment — stays structural (its lowering is not
  string-keyed; it's a dispatch to `subscript_set` per
  `docs/mutable-subscript-design.md` §4.2).
- `LoopKind::Iterator` — stays structural until the `Iterator` trait/lang-item
  is designed and the body desugaring is specified.
- Everything else (`Bin`, `Unary`, `Field`, `Index`, `Block`, `If`, `Match`,
  `Loop`, `StructLit`) — not sugar, passes through unchanged.

The desugar pass is **narrow by design**: it only rewrites variants that have
an entry in the `SUGAR_HANDLERS` table (see §3.1). Adding a new sugar =
adding one row to the table.

## 3. Implementation — step-by-step

### Step 1: Create `axiom-hir/src/desugar.rs` — [file: new]

The core module. Contents:

- `DesugarResult` struct
- `fresh_id()` / `fresh_temp_name()` helpers
- `desugar_expr(e, lang_items, next_id, temp_counter) -> Option<Expr>`
  — returns `Some(replacement)` if `e` is sugar, `None` if it passes through
- `desugar_block(b, lang_items, next_id, temp_counter)`
- `desugar_stmt(s, lang_items, next_id, temp_counter)`
- `desugar_items(items, lang_items, next_id, temp_counter)`
- `pub fn desugar(hir, lang_items) -> DesugarResult` — the public entry point

The `desugar_expr` function uses a match/dispatch table approach:

```rust
fn desugar_expr(
    e: &mut Expr,
    lang_items: &LangItems,
    next_id: &mut usize,
    temp_counter: &mut usize,
) {
    // Recurse into sub-expressions first
    desugar_subexprs(e, lang_items, next_id, temp_counter);
    // Then check if this variant is sugar
    let replacement = match e {
        Expr::ListLit(lit) => desugar_list_lit(lit, lang_items, next_id, temp_counter),
        _ => None,
    };
    if let Some(new_expr) = replacement {
        *e = new_expr;
    }
}
```

Recursing *before* desugaring the current node means nested `[[a], [b]]`
desugars correctly (inner list literal becomes a `Block`, then the outer).

### Step 2: Wire into `check_modules` — [file: axiom-typeck/src/lib.rs]

In `check_modules`, after `resolve_lang_items` (line 103) and before
`check_with_lang_items` (line 110), insert:

```rust
// Determine the highest HirId across all items for fresh-id seeding.
let max_id = all_items.iter()
    .flat_map(|item| collect_hir_ids(item))
    .max()
    .unwrap_or(HirId(0))
    .0;
let mut next_id = max_id + 1;
axiom_hir::desugar::desugar(&mut hir, &lang_items, &mut next_id);
```

`collect_hir_ids` is a new helper that walks items and returns all `HirId`s.
(Alternative: thread `next_id` through from `lower_structural` — simpler but
requires plumbing. The walk approach is self-contained and correct.)

**Important:** typeck's `check_with_lang_items` still receives `lang_items`
because it needs them for other purposes (empty-list type annotation fallback
during the transition, and future lang-item lookups). But `infer_list_lit` is
removed.

### Step 3: Remove `infer_list_lit` from typeck — [file: axiom-typeck/src/typeck/methods.rs]

The `infer_list_lit` method (lines ~570–607) is deleted. After the desugar pass,
`Expr::ListLit` no longer exists in the HIR, so the method is dead code.

Also remove:
- The `Expr::ListLit` match arm in `infer_expr` / statement handlers (typeck
  will now see `Block` expressions with `Call` + `MethodCall` nodes instead).
- The `try_annotated_empty_list` logic — empty `[]` is now desugared to a
  `Call(List::new, [])` and typed normally.

### Step 4: Remove `lower_list_lit` from IR — [file: axiom-ir/src/lower/expr.rs]

The `lower_list_lit` function (lines ~314–345) is deleted. After the desugar
pass, list literals arrive as normal `Call`/`MethodCall` nodes and are lowered
by the existing `lower_call`/`lower_method_call` code paths.

Also remove:
- The `Expr::ListLit` match arm in `lower_expr`.
- The `LIST_NEW`/`LIST_WITH_CAPACITY`/`LIST_PUSH` constants used only by
  `lower_list_lit` (they stay in `lang.rs` for the *desugar pass* to use — but
  verify the IR no longer imports them).

### Step 5: Remove `Expr::ListLit` from HIR — [file: axiom-hir/src/hir/mod.rs]

Since nothing downstream needs `ListLit` anymore, it can be removed from
the `Expr` enum. This cascades:

- Remove `ListLitExpr` struct (or keep it as a desugar-internal type if the
  desugar module wants to reference it — but it no longer needs to be in the
  public HIR enum).
- Update all `match` arms across `axiom-hir` (serialize, resolve, coverage,
  lower, fuzz, lib) that match on `Expr::ListLit`.
- Update `ALL_EXPR_VARIANTS` in `desugar_coverage.rs` (one fewer variant).

**Decision: keep `ListLitExpr` as an internal type in `desugar.rs`** — the
desugar pass matches on it to trigger the rewrite. The `Expr` enum drops
`ListLit`, but `ListLitExpr` lives as a desugar-internal struct (or the
desugar pass matches on it before it's removed from the tree). Actually,
simpler: the desugar pass walks the HIR *after* lowering, so `Expr::ListLit`
exists at the start of the pass but is gone by the end. After Step 5, `ListLit`
is removed from the `Expr` enum and `ListLitExpr` is removed from `hir/mod.rs`.

Wait — the **lowering** pass (`axiom-hir/src/lower/expr.rs`) produces
`Expr::ListLit`. If we remove it from the enum, lowering breaks. So the
sequence is:

1. The desugar pass rewrites all `Expr::ListLit` nodes to `Block` expressions.
2. After desugaring, no `Expr::ListLit` remains.
3. All downstream stages (typeck, coverage, serialize in axiom-hir, IR
   lowering) no longer need to handle `Expr::ListLit`.
4. **But** the HIR *lowering* (AST→HIR) still produces `Expr::ListLit` — the
   parser emits `K::ListLit`, and `lower/expr.rs` converts it to
   `Expr::ListLit(ListLitExpr { ... })`.

So the correct approach: keep `Expr::ListLit` in the enum (the lowerer produces
it), but remove all *handling* of it from stages after the desugar pass. The
coverage invariant changes from "ListLit is sugar" to "ListLit must be gone
after desugar pass" — a new invariant that checks post-desugar HIR contains no
`Expr::ListLit`.

### Step 6: Update the coverage invariant — [file: axiom-ir/tests/desugar_coverage.rs]

The current invariant says "every sugar `Expr` variant has a desugaring golden."
After the move, `ListLit` is no longer in the IR at all — it's desugared in HIR.
The IR golden tests become irrelevant for `ListLit`.

Changes:
1. Remove `ListLit` from `ALL_EXPR_VARIANTS` (it's gone from IR-level `Expr`…
   actually wait — IR doesn't have its own `Expr` enum; it uses `IrInstr`.
   The coverage test mirrors `axiom_hir::Expr` variants. Since `ListLit` stays
   in the HIR enum (it's created by the lowerer, consumed by the desugar pass),
   `ALL_EXPR_VARIANTS` still lists it. But the *sugar* classification changes:
   `ListLit` is not "sugar → golden" anymore — it's "structural → desugared by
   HIR pass → never reaches IR."
2. Update `SUGAR_EXPRS` to remove the `ListLit` sugar spec.
3. Add a **new invariant**: the `desugar` pass must eliminate every
   `Expr::ListLit`. A test that runs `desugar()` on a known HIR containing
   `ListLit` and asserts no `ListLit` remains — then re-serializes and verifies
   the output is a valid HIR with `Block`/`Call`/`MethodCall` nodes.
4. Add golden HIR snapshots *after* desugaring (not IR snapshots) — a `.hir`
   snapshot showing the desugared form. Template: the existing
   `multi_file_golden` HIR snapshot tests.
5. Existing IR golden `list_literal.ir` — keep it, but it's now produced by
   normal call lowering, not by `lower_list_lit`. It should be identical or
   near-identical to the current golden. If it differs, update it.

### Step 7: Add HIR desugar goldens — [new files]

New golden directory: `crates/axiom-hir/tests/desugar_goldens/` with `.hir`
files showing the desugared form. Regenerated with `UPDATE_SNAPSHOTS=1`.

Example `list_literal.hir`:
```
Block(<id>) {
  VarStmt(<id>) __list_0: = Call(<id>) with_capacity→<def_id>(Lit(<id>) Int(3))
  ExprStmt(<id>) MethodCall(<id>) .push(Path(<id>) __list_0→<def_id>, Lit(<id>) Int(10))
  ...
  tail: Path(<id>) __list_0→<def_id>
}
```

### Step 8: Remove dead constants from `lang.rs` — [file: axiom-hir/src/lang.rs]

The constants `LIST_NEW`, `LIST_WITH_CAPACITY`, `LIST_PUSH` were originally for
IR lowering's string-keyed dispatch. The desugar pass uses the *lang-item*
`DefId`s (`list_new`, `list_with_capacity`, `list_push`), not the name strings.

- Keep: `LANG_LIST`, `LANG_LIST_NEW`, `LANG_LIST_WITH_CAPACITY`, `LANG_LIST_PUSH`
  (the lang-item keys, used by the registry and desugar pass).
- Keep: `LIST`, `SUBSCRIPT`, `SUBSCRIPT_SET` and the `subscript_fn` helpers
  (used by subscript lowering, which stays name-convention for now).
- Remove: `LIST_NEW`, `LIST_WITH_CAPACITY`, `LIST_PUSH` (the qualified string
  constants for IR lowering — no longer referenced after Step 4).
- Update: `test_no_raw_qualified_list_strings_outside_lang_module` to remove
  the banned list (those strings aren't magic anymore, since they're not used
  anywhere).

### Step 9: Run existing tests — fix failures

- `list_e2e.rs` — must pass unchanged (the rendered output is the same).
- `collections` tests — same.
- `lang_items.rs` tests — same (lang items still resolve).
- `desugar_coverage.rs` — updated per Step 6.
- `multi_file_golden` HIR snapshots — may need regeneration (ListLit nodes
  replaced by Block nodes in HIR dumps after desugaring).
- `invariants.rs` — no change expected.
- `subscript` tests — no change expected.
- All typeck unit tests — `infer_list_lit` tests removed; list-literal typing
  is now tested through the desugared form.

## 4. Test harness — the six layers

Following `lang-items-and-desugaring-design.md` §6.1 and
`lexer-testing.md` §4, applied to the new pass:

### 4.1 Unit tests — [file: axiom-hir/src/desugar.rs, `#[cfg(test)] mod tests`]

- `test_desugar_empty_list_becomes_new_call` — `[]` → `Call(new, [])`
- `test_desugar_singleton_list` — `[42]` → `with_capacity(1)` + `push(42)`
- `test_desugar_multi_element_list` — `[1, 2, 3]` → `with_capacity(3)` + 3 pushes
- `test_desugar_nested_list` — `[[1], [2, 3]]` → inner desugared first, then outer
- `test_desugar_list_in_call_arg` — `f([a, b])` → temp hoisted before call
- `test_desugar_does_not_touch_non_sugar` — `Bin`, `If`, `Loop` etc. pass through
- `test_desugar_generates_unique_ids` — every fresh node has a unique HirId
- `test_desugar_uses_lang_item_def_ids` — the `Call` callee's `DefId` matches
  the lang-item registry
- `test_desugar_result_has_no_list_lit` — post-desugar walk finds zero
  `Expr::ListLit`

These tests build HIR nodes by hand (like the existing `lang::tests` do via
`axiom_parser::parse` + `lower_structural`), run `desugar_expr`/`desugar`, then
assert on the resulting tree.

### 4.2 Golden snapshots — [new directory: axiom-hir/tests/desugar_goldens/]

Full `.hir` serialized snapshots of the desugared output for key sugar forms.
Regenerated with `UPDATE_SNAPSHOTS=1`. Pinned in CI.

| Golden file | Driver |
|---|---|
| `list_literal.hir` | `fn main() { val xs = [10, 20, 30] }` |
| `empty_list_annotated.hir` | `fn main() { val xs: List<Int> = [] }` |
| `nested_list.hir` | `fn main() { val xs = [[1], [2, 3]] }` |

### 4.3 Coverage invariant — [update: axiom-ir/tests/desugar_coverage.rs]

Updated per Step 6 above. The new invariant: the `desugar` pass eliminates every
`Expr::ListLit` from the HIR.

Additionally, a new `#[test]` in `axiom-hir` walks the `Expr` enum variants
(like `ALL_EXPR_VARIANTS`) and asserts that every variant either:
- passes through desugaring unchanged (non-sugar), or
- is handled by a desugar rule (sugar).

Adding a new `Expr` variant without updating this test fails the build.

### 4.4 End-to-end — [existing: list_e2e.rs, collections tests]

All existing e2e and integration tests must pass unchanged. The desugar pass
is a behaviour-preserving refactor.

### 4.5 Diagnostics — [update: lang_items.rs]

- `MissingLangItem { key: "list" }` — still fires if stdlib is missing the
  `@lang("list")` tag.
- `LangItemOutsideStdlib` — still fires if user code has `@lang`.
- No new diagnostics needed (the desugar pass doesn't add errors; it transforms
  tree shapes).

### 4.6 Fuzz/property — [new or extended: axiom-hir/tests/fuzz.rs]

- **Re-typecheck invariant:** desugar a random-but-valid HIR containing
  `ListLit` nodes, then typeck the result — must not introduce diagnostics that
  weren't present before desugaring (on valid inputs).
- **Idempotence:** running `desugar` twice on the same HIR produces identical
  output (desugaring an already-desugared tree is a no-op).

## 5. Rollback plan

Every step is independently revertible:

1. The `desugar.rs` module is added — remove the file, remove the call in
   `check_modules`, and the old `infer_list_lit` + `lower_list_lit` are still
   there.
2. `check_modules` wiring — revert the insertion, and behaviour is unchanged.
3. Typeck `infer_list_lit` removal — revert the deletion.
4. IR `lower_list_lit` removal — revert the deletion.
5. `Expr::ListLit` removal — revert (though this is the hardest to revert,
   defer it until all tests pass with the new code; leave `Expr::ListLit` in
   the enum with a `#[allow(dead_code)]` annotation until final cleanup).

**Safety valve:** if any e2e test breaks and can't be fixed within a reasonable
time, stop and revert. The old code is not deleted until the new code passes
the full suite.

## 6. Implementation estimate

| Step | Effort | Risk |
|---|---|---|
| 1. `desugar.rs` core | Medium — ~200 lines, pure transform | Low — unit-testable in isolation |
| 2. `check_modules` wiring | Small — ~10 lines | Low — one insertion point |
| 3. Remove `infer_list_lit` | Small — delete ~40 lines | Medium — typeck regressions possible |
| 4. Remove `lower_list_lit` | Small — delete ~40 lines | Low — IR lowering is self-contained |
| 5. `Expr::ListLit` handling | Medium — ~10 files touched, match arm removal | Low — compiler errors guide the cleanup |
| 6. Coverage invariant update | Small — ~30 lines | Low |
| 7. Golden snapshots | Small — new files + generator | Low |
| 8. Dead constant removal | Small — ~15 lines | Low |
| 9. Run & fix existing tests | Medium | High — the critical gate |

**Total: ~3–4 sessions of focused work.** The core risk is typeck regressions
(Step 3) — the rest is mechanical.

## 7. Open decisions

| # | Question | Lean |
|---|---|---|
| D1 | Should `Expr::ListLit` be removed from the enum, or kept with dead-code allow? | **Keep it** during implementation (desugar pass matches on it). Remove from the `Expr` enum only as the final cleanup step after all tests pass. |
| D2 | Should the `desugar` pass run on the `Hir` or on each module's `items` separately? | **On the combined `Hir`** (post-merge in `check_modules`). Simpler — one walk, one ID counter. |
| D3 | Should `VarStmt` vs `ValStmt` for the temp? | **`VarStmt`** — `push` takes `inout self`, typeck requires mutable. |

## 8. Cross-references

- `lang-items-and-desugaring-design.md` §3.2/§3.3 (lang-item infrastructure — prerequisite, done)
- `lang-items-and-desugaring-design.md` §4 (the trigger — this doc supersedes it)
- `lang-items-and-desugaring-design.md` §6 (harness — this doc instantiates it)
- `DESIGN_SPEC.md` §7.1 (loop forms — `loop x in` will be next sugar)
- `ir-design.md` (current `lower_list_lit`)
- `mutable-subscript-design.md` §4.2 (indexed assign — stays, not desugared yet)
- `hir-testing.md` (existing HIR snapshot template)
