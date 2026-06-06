# Subscript v0 Fix — Design Plan

> **Status: Proposed; not yet implemented.**
> This plan fixes the three structural problems in v0's subscript model
> without introducing new syntax (`get`/`set` blocks were considered and
> [rejected](https://codeberg.org/sabinbir/axiom/commit/8d5bb64)). The fix
> makes `self` explicit, derives read/write from `let`/`inout` conventions,
> and adds duplicate-detection. Yield is deferred to v1.

## 0. The problem

The current v0 subscript model has three structural gaps:

### Gap 1 — `self` is invisible

The user writes:
```axiom
// current syntax
subscript(index: Int) -> T { self.buf[index] }
subscript(index: Int, value: T) { self.buf[index] = value }
```

But `self` is **not in the parameter list**. The HIR lowerer synthesizes it —
inspecting the param count and guessing read vs write. This breaks the rule
that every parameter convention (`let`/`inout`/`sink`) is **visible in source**.

A user reading the declaration can't tell from the signature alone whether
`self` is borrowed `let` or `inout`. They must reason about param count and
return type — conventions the compiler asserts by fiat, not by parsing what
the user wrote.

### Gap 2 — Silent-duplicate subscript

If an impl has two identical read subscripts:
```axiom
subscript(index: Int) -> T { self.buf[index] }
subscript(index: Int) -> T { self.buf[index] }   // silently ignored
```
The typeck resolver uses `Iterator::find()` — returns the first match, ignores
the second. No diagnostic. The user thinks both are active.

### Gap 3 — `is_setter` is derived heuristically

`lower_subscript_def` in `crates/axiom-hir/src/lower/item.rs:440` decides
read vs write by checking if the subscript has a return type. This works
for the current dual-declaration pattern but breaks if someone writes a
single declaration with side effects and no return type. It's a guess, not
a mechanical property of the AST.

## 1. The fix

### 1.0 Make `self` explicit

**Before:**
```axiom
subscript(index: Int) -> T { self.buf[index] }
subscript(index: Int, value: T) { self.buf[index] = value }
```

**After:**
```axiom
subscript(self, index: Int) -> T { self.buf[index] }
subscript(inout self, index: Int, value: T) { self.buf[index] = value }
```

`self` is now a regular parameter with a borrowing convention:
- `self` (or `let self`) → read-only, `get` path
- `inout self` → mutable, `set` path

This matches `fn` methods, where the user already writes:
```axiom
fn push(inout self, item: T) { self.list.add(inout item) }
fn size(self) -> Int { self.len }
```

### 1.1 Derive read/write from `self` convention

| `self` convention | Subscript kind | Lowered name |
|---|---|---|
| `let self` (or bare `self`) | Read | `subscript_fn` |
| `inout self` | Write (if `value` param present) | `subscript_set_fn` |

The lowerer no longer guesses from param count. It inspects the `SelfParam`
AST node: `is_inout()` → write, otherwise → read.

`is_setter` stays as a boolean on `SubscriptDef` — but it's **derived** from
the parsed `self` convention, not inferred from surrounding syntax.

### 1.2 Add duplicate-detection diagnostic

`find_impl_subscript` in `crates/axiom-typeck/src/typeck/methods.rs:236`
currently uses `.find()`. Replace with:

1. **Collect** all subscripts matching the index-param count.
2. **Partition** into read (`let self`) and write (`inout self`).
3. If either partition has **>1 entry** → emit diagnostic: *"Duplicate subscript
   with same index-param signature. Only one read and one write subscript is
   allowed per index shape."*

This is a Hard guard — see §3 H6.

### 1.3 No synthesized `self` in HIR lowerer

The lowerer must **not** create a `self` parameter where none exists in the AST.
It parses `self` from the subscript's parameter list just like it does for `fn`
methods. The code path that currently adds `self` to subscript params is removed.

A compile-fail test asserts: if an AST node for a subscript has no `SelfParam`,
the lowerer panics (or produces a clear error — we can decide during impl).

### 1.4 Shorthand: read-only subscript (no `value` param)

If a subscript has `let self` and **no** `value` parameter, it's a read-only
subscript. This is the common case for lookups (`map[key]`):

```axiom
subscript(self, key: K) -> V { self.inner.get(inout key) }   // read-only
```

The typeck layer must still error if the user tries `map[key] = val` on this
subscript — the diagnostic `NoWritableSubscript` already exists and is correct.

## 2. Syntax rules (parser changes)

### 2.0 Existing grammar

```
ImplMember  = FunctionDef | SubscriptDef | InitDef
SubscriptDef = 'subscript' '(' ParamList? ')' ( '->' Ty )? Block
```

`SelfParam` is never in `ParamList` for subscripts — it's synthesized later.

### 2.1 New grammar

```
SubscriptDef = 'subscript' '(' SelfParam ',' ParamList ')' ( '->' Ty )? Block
```

`SelfParam` is **required** as the first parameter:
```
SelfParam = 'inout'? 'self'   // 'self' or 'inout self'
```

The parser must reject a `subscript` without `self` as the first param with a
clear error: *"Subscript must declare `self` (or `inout self`) as its first
parameter."*

### 2.2 AST changes

`SubscriptDef` in `crates/axiom-parser/src/ast/item.rs:369` gains:
```rust
fn self_param(&self) -> Option<SelfParam>;  // returns first param if it's SelfParam
fn is_inout(&self) -> bool;                  // derived from self_param
```

The lowerer reads these instead of guessing.

## 3. Harness & drift guards

### H3' — Subscript-shape coverage matrix (Hard)

The `output_assertion_guard` test already covers every (base kind × operator)
combination. Extend it to also cover every (base kind × `let self` × `inout self`).
A test with two identical `let self` subscripts must produce the new duplicate
diagnostic (negative test, `.stderr` golden).

### H6 — No silent duplicate subscript (Hard)

An impl with two `subscript(self, index: Int) -> T` declarations (same index
shape, same `self` convention) must produce a compile error. Never silently
pick one and ignore the other.

Test: add a program with two identical read subscripts, assert it fails to
compile with the expected error message. `.stderr` golden.

### H7 — No synthesized `self` (Hard)

The HIR lowerer must not create a `self` parameter where none exists in the AST.
A source-scan test (or a compile-fail test with a missing `self` param) fails the
build if the lowerer synthesizes a `self` param from a non-`SelfParam` AST node.

### H8 — Removed heuristic `is_setter` (Hard)

After migration, `is_setter` must be derived from the parsed `SelfParam`
convention, not from param count or return-type presence. A compile-fail test
that constructs `SubscriptDef { is_setter: true }` without an `inout self`
param guarantees the field is mechanically derived.

### H9 — Existing programs produce identical output (Hard)

After all `.ax` files are migrated, all showcase and stdlib programs must
produce bit-identical output compared to pre-fix:

```bash
cargo run -p axiom-cli -- run showcase/showcase.ax
cargo run -p axiom-cli -- run showcase/place_assignment.ax
```

Trace goldens must not change. This is a regression gate.

### What's mechanized vs judgment

| Guarantee | Mechanism | Strength |
|---|---|---|
| `self` convention visible in source | Parser requires `SelfParam` | **Hard** |
| No silent duplicate subscripts | H6: typeck diagnostic | **Hard** |
| Write on read-only subscript errors | Existing `NoWritableSubscript` diagnostic + `.stderr` golden | **Hard** |
| No synthesized `self` | H7: compile-fail test | **Hard** |
| `is_setter` derived, not guessed | H8: compile-fail on heuristic path | **Hard** |
| Every (shape × arity × op) combo covered | H3' coverage matrix + drift guard | **Hard** |
| Existing programs unbroken | H9: output-identical golden gate | **Hard** |
| Semantics match DESIGN_SPEC §4.4.1 intent | Review against spec | **Soft at the margin** |

## 4. What this does NOT change

- **No `yield`.** Yield-based projection requires Perceus + coroutine support
  (v1). This fix is v0 only — a safety/correctness cleanup of the existing
  dual-declaration model.

- **No `get`/`set` blocks.** The Swift-style single-declaration-with-blocks
  approach was considered and rejected. Two declarations with explicit `self`
  is the simpler path: it reuses `self`/`inout self` conventions that `fn`
  methods already have, requires no new syntax, and maps directly to the
  existing `subscript_fn`/`subscript_set_fn` IR lowering.

- **No change to IR or VM.** The lowered function names (`subscript_fn`,
  `subscript_set_fn`) are unchanged. The `IndexSet`/`Index` instructions
  are unchanged. The `UnsupportedIndexBase` guard stays.

- **No change to the desugar.** `a[i] += 1` still desugars to
  `temp = a[i]; temp += 1; a[i] = temp`. The only difference is that
  `a[i]` and `a[i] = temp` now call functions whose `self` convention was
  parsed from source rather than synthesized.

## 5. Yield — v1 path (not part of this fix)

Yield-based projection is the DESIGN_SPEC's end state (§4.4.1). It is **strictly
additive** over the fixed two-declaration model:

```axiom
// v1: add yield to the existing pair (or replace the pair entirely)
subscript(inout self, index: Int) -> inout T {
    yield &self.buf[index]
}
```

When a type has a `yield` subscript, the compiler prefers it for `inout` access
(in-place projection, zero copies). When it doesn't, the compiler falls back to
the `get`/`set` pair (read-compute-write). All v0 subscript code continues to
compile and run correctly; yield is an opt-in optimization.

### What yield enables that the v0 fix does not

- O(1) in-place mutation without copy-out/copy-back
- True projection semantics (borrow part of a value through an abstraction
  boundary)
- Same speed as raw pointer access with no unsafe code

### Why yield is deferred

- Requires the Perceus compile-time refcounting pass (v1)
- Requires coroutine support in the IR (suspension/resumption points)
- Requires the exclusivity pass to verify that yielded `inout` projections
  don't alias (§4.3)
- The v0 two-declaration model is correct, safe, and complete — yield is
  strictly an optimization

## 6. Migration plan

### 6.1 Files by layer (dependency order)

#### Stage A — Parser

| File | Change |
|---|---|
| `crates/axiom-parser/src/syntax_kind.rs` | No new node kinds needed |
| `crates/axiom-parser/src/grammar/item.rs:353` | `subscript_def()`: require `SelfParam` as first param; reject subscripts without it |
| `crates/axiom-parser/src/ast/item.rs:369` | Add `SubscriptDef::self_param()`, `SubscriptDef::is_inout()` |
| `crates/axiom-parser/src/ast/item_part.rs:564` | `impl_members.subscripts()` — no change |
| `crates/axiom-parser/src/ast/tests.rs:20` | Update AST node coverage |

#### Stage B — HIR

| File | Change |
|---|---|
| `crates/axiom-hir/src/hir/items.rs:126` | `SubscriptDef.is_setter` — now derived from parsed `SelfParam` convention. No other struct changes. |
| `crates/axiom-hir/src/lower/item.rs:440` | `lower_subscript_def()`: stop synthesizing `self`. Read `SelfParam` from AST. Derive `is_setter` from it. If no `SelfParam`, emit clear error (or panic — the parser should prevent this path). |
| `crates/axiom-hir/src/serialize/mod.rs` | No structural change needed (`is_setter` serialization unchanged) |
| `crates/axiom-hir/src/resolve/item.rs` | No change — resolves subscript body as before |
| `crates/axiom-hir/src/lang.rs` | `SUBSCRIPT_SET`, `subscript_set_fn()` — no change |
| `crates/axiom-hir/src/lib.rs` | No export changes |
| `crates/axiom-hir/tests/invariants.rs` | Update variant coverage |
| `crates/axiom-hir/tests/fuzz.rs` | Update fuzz generator to include explicit `self` |
| `crates/axiom-hir/tests/fixtures/modules/*/main.hir` | Regenerate HIR goldens (`UPDATE_SNAPSHOTS=1`) |

#### Stage C — Typeck

| File | Change |
|---|---|
| `crates/axiom-typeck/src/typeck/methods.rs:236` | `find_impl_subscript()` / `find_impl_write_subscript()`: add duplicate detection. Partition by `self` convention, error if >1 in either partition. |
| `crates/axiom-typeck/src/typeck/mod.rs` | No structural change |
| `crates/axiom-typeck/src/coverage.rs` | No structural change |
| `crates/axiom-typeck/src/serialize/mod.rs` | No change |
| `crates/axiom-typeck/tests/diagnostics.rs` | Add test: two identical read subscripts → duplicate error. Existing `NoWritableSubscript` test unchanged. |
| `crates/axiom-typeck/tests/builtin_traits.rs` | Verify no regression |

#### Stage D — IR lowering

| File | Change |
|---|---|
| `crates/axiom-ir/src/lower/item.rs` | `lower_subscript_def()`: `is_setter` is now derived, not guessed. No logic change needed. |
| `crates/axiom-ir/src/lower/expr.rs:233` | `lower_index_read` / `lower_index_write` — no change |
| `crates/axiom-ir/src/lower/assign.rs` | No change |
| `crates/axiom-ir/tests/desugar_coverage.rs` | No change |
| `crates/axiom-ir/tests/desugar_goldens/index_assign.ir` | No change expected |

#### Stage E — VM

| File | Change |
|---|---|
| `crates/axiom-vm/src/exec/instr.rs` | No change |
| All VM test `*.rs` files | Update inline `.ax` programs to include explicit `self` in subscript declarations |

#### Stage F — Stdlib & showcase (.ax source)

| File | Change |
|---|---|
| `stdlib/std/collections/list.ax:73` | `subscript(index: Int) -> T` → `subscript(self, index: Int) -> T` |
| `stdlib/std/collections/list.ax:82` | `subscript(index: Int, value: T)` → `subscript(inout self, index: Int, value: T)` |
| `stdlib/std/collections/map.ax:176` | `subscript(key: K) -> V` → `subscript(self, key: K) -> V` (read-only) |
| `showcase/showcase.ax` | No syntax change (only uses subscript at call site) |
| `showcase/place_assignment.ax` | Add explicit `self`/`inout self` to Grid's subscript declarations |

### 6.2 Migration verification gate

```bash
# 1. All existing stdlib/showcase programs still run and produce identical output
cargo run -p axiom-cli -- run showcase/showcase.ax
cargo run -p axiom-cli -- run showcase/place_assignment.ax

# 2. Full pre-commit gate
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test

# 3. HIR goldens regenerated
UPDATE_SNAPSHOTS=1 cargo test -p axiom-hir --test multi_file_golden

# 4. Trace goldens unchanged (no VM change)
UPDATE_SNAPSHOTS=1 cargo test -p axiom-vm --test golden

# 5. All new H6/H7/H8 guard tests pass
cargo test -p axiom-typeck --test diagnostics
cargo test -p axiom-vm --test invariants
cargo test -p axiom-vm --test output_assertion_guard

# 6. New duplicate-detection test passes
cargo test -p axiom-typeck --test diagnostics -- duplicate_subscript
```

## 7. Checklist — TDD order (red first)

- [ ] **1. Parser:** Require `self`/`inout self` as first param in `subscript_def`
- [ ] **2. Parser:** AST accessors (`self_param()`, `is_inout()`)
- [ ] **3. Parser:** Diagnostic for missing `self` in subscript
- [ ] **4. HIR:** `lower_subscript_def` — stop synthesizing `self`, read from AST
- [ ] **5. HIR:** `is_setter` derived from parsed `SelfParam`
- [ ] **6. Typeck:** Duplicate-detection diagnostic (H6) — two same-convention subscripts with same index shape → error
- [ ] **7. Typeck:** Existing `NoWritableSubscript` diagnostic still fires
- [ ] **8. `.ax` stdlib:** Migrate `list.ax`, `map.ax` to explicit `self`
- [ ] **9. `.ax` showcase:** Migrate `place_assignment.ax` Grid subscript
- [ ] **10. Test:** H7 guard — compile-fail test for missing `self`
- [ ] **11. Test:** H8 guard — verify `is_setter` derived mechanically
- [ ] **12. Test:** H6 guard — duplicate subscript → error (`.stderr` golden)
- [ ] **13. Test:** H9 regression gate — all existing programs output unchanged
- [ ] **14. HIR goldens:** Regenerate with `UPDATE_SNAPSHOTS=1`
- [ ] **15. Docs:** Update `DESIGN_SPEC.md` §4.4.1 v0 implementation note
- [ ] **16. Mark doc status `[x]` and commit**

## 8. Cross-references

| Source file | Role |
|---|---|
| `DESIGN_SPEC.md:267` | §4.3 — exclusivity rule (applies to `inout self` in subscripts) |
| `DESIGN_SPEC.md:287-295` | §4.4.1 — end-state `yield` subscript design |
| `DESIGN_SPEC.md:305-313` | §4.4.1 — v0 interim setter-desugar note |
| `docs/mutable-subscript-design.md` | Prior v0 fix doc (setter-desugar, `UnsupportedIndexBase` guard) |
| `docs/vm-design.md` | VM design — subscript lowering details |
| `docs/ir-design.md:§3.3.1` | `lower_index_read`/`lower_index_write` helpers |
| `crates/axiom-parser/src/grammar/item.rs:353` | `subscript_def()` grammar function |
| `crates/axiom-hir/src/hir/items.rs:126` | `SubscriptDef` struct |
| `crates/axiom-hir/src/lower/item.rs:440` | `lower_subscript_def` — synthesizes `self` (to be removed) |
| `crates/axiom-typeck/src/typeck/methods.rs:236` | `find_impl_subscript` — uses `.find()` (to be hardened) |
| `stdlib/std/collections/list.ax:73,82` | List subscript — read + write |
| `stdlib/std/collections/map.ax:176` | Map subscript — read only |
| `showcase/place_assignment.ax` | Grid subscript — read + write |
