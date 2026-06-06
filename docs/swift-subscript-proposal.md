# Swift-style `get`/`set` Subscript — Design Proposal

> **Status: Proposed; nothing built.** This proposes replacing the current v0
> two-declaration subscript model (`SubscriptDef` with `is_setter`) and the spec's
> `yield`-based end-state (§4.1) with a **single Swift-style `get`/`set` subscript**
> that reuses Axiom's existing `let`/`inout` calling conventions. This doc records
> *why*, *what changes*, the *staged plan*, and the *drift-guard harness* that
> stops the known v0 silent-failure classes from recurring.

## 0. The concern this answers

The current v0 subscript has three structural problems surfaced during implementation
(see `docs/mutable-subscript-design.md` and the showcase review):

1. **Two declarations masquerade as overloading.** Read and write are separate
   `subscript` declarations distinguished by the presence/absence of a return type
   arrow. A user who defines two read subscripts gets no diagnostic — the second is
   silently ignored (`find_impl_subscript` uses `.find()`). Same for two write
   subscripts. The source-code appearance suggests Axiom has function overloading
   when it does not.

2. **`inout self` is invisible.** The writer subscript mutates `self` through an
   `inout` convention synthesized by the HIR lowerer — the user never writes `inout`
   in the signature. Meanwhile every `fn` method that mutates `self` requires
   explicit `fn push(inout self, ...)`. This trains users to ignore the convention
   they need to learn for methods.

3. **Write-in-read hole.** A one-subscript design (Spec §4.4) places both read and
   write semantics behind a single body. Nothing prevents `self.buf[i] = bad_value`
   from executing during a `val x = base[i]` read unless the borrow checker or
   structural validation catches it. The `get`/`set` split eliminates this class of
   bug at the syntax level.

4. **`yield` semantics are heavy for v0.** The spec's end-state (§4.1) requires
   suspend/resume projection machinery coupled to the Perceus memory model, which
   lands in v1. `get`/`set` blocks reuse existing IR infrastructure (method calls
   with `let`/`inout` self) — no new VM instruction, no new semantic concept.

## 1. The proposal — syntax and semantics

### 1.0 Declared once, used both ways

```rust
// ── read + write subscript ──
subscript(index: Int) -> T {
    get(self) { self.buf[index] }
    set(inout self, value: T) { self.buf[index] = value }
}

// ── read-only subscript (shorthand — no `set` block) ──
subscript(index: Int) -> T { self.buf[index] }

// ── multi-param subscript ŌöĆ
subscript(row: Int, col: Int) -> T {
    get(self) { self.grid[row * self.cols + col] }
    set(inout self, value: T) { self.grid[row * self.cols + col] = value }
}

// ── call sites ŌöĆ
val a = xs[0]         // read → dispatches to `get` block
xs[0] = 42            // write → dispatches to `set` block
xs[0] += 1            // compound → read via `get`, binop, write via `set`
grid[1, 2] = 99       // multi-param write
```

### 1.1 Semantic rules

| Rule | Enforcement |
|---|---|
| `get` block must have `self` as first param (defaults to `let`) | Parser / HIR lowerer |
| `set` block must have `inout self` as first param | Parser / HIR lowerer |
| `set` block must have `value: T` as last param, where `T` matches return type | Typeck |
| `get` block may not mutate `self` (borrow-checked against `let` convention) | Typeck / borrow pass |
| `set` block may mutate `self` via `inout` write-back | Already works (same as `fn push`) |
| At most one `get` block and one `set` block per subscript declaration | Parser (structural) |
| A subscript without a `set` block is read-only | Parser → `SubscriptDef.has_set = false` |
| Multi-param subscripts: all index params before the return arrow appear at call site | Parser + lowering |

### 1.2 How this aligns with MVS

| Convention | Where it appears | Why |
|---|---|---|
| `let self` | In `get(self)` block (written or defaulted) | Reading doesn't mutate |
| `inout self` | In `set(inout self, ...)` block (explicit) | Writing mutates — same as `fn push(inout self)` |
| `value: T` | In `set` block (explicit, typed) | No implicit `newValue` — Axiom is explicit |

The subscript is a **single declaration** with two borrowing contexts. This is
conceptually simpler than the spec's `yield` (suspend/lend/resume) and mechanically
simpler than the current v0 (two HIR items with synthesized `self`). It reuses
exactly one concept not yet in use: `get`/`set` as structural keywords within a
`subscript` body.

## 2. HIR changes

### 2.0 Current state (v0): two `SubscriptDef`s

```rust
struct SubscriptDef {
    id: HirId,
    params: Vec<Param>,          // synthesized `self` + user params
    return_type: Option<HirTy>,
    body: Block,
    is_setter: bool,             // true = write, false = read
}
// A type with read+write carries two SubscriptDefs in the impl.
```

### 2.1 Proposed: one `SubscriptDef`, two bodies

```rust
struct SubscriptDef {
    id: HirId,
    index_params: Vec<Param>,          // user-written index params (no `self`, no `value`)
    return_type: HirTy,                // always present (required for set value type)
    get_body: Block,                   // `get(self) { ... }` body; synthesized `let self`
    set_body: Option<SetBody>,         // `set(inout self, value: T) { ... }`; None = read-only
    /// True if the user wrote an explicit `get` block (vs. shorthand).
    has_explicit_get: bool,
}

struct SetBody {
    value_param_id: HirId,             // the synthesized `value` param
    body: Block,                       // setter body with `inout self` synthesized
}
```

Key changes from v0:
- `return_type` is no longer `Option` — always present (even for write-only subscripts it defines the value type).
- `is_setter` removed — the distinction is structural (presence of `set_body`).
- `get_body` / `set_body` replace the single `body`.
- Only one `SubscriptDef` per (return type, index param count) — no more silent duplicates.

### 2.2 AST → HIR lowering changes

The parser gains `get`/`set` as contextual keywords inside `subscript` bodies. The
lowerer:

1. Parses index params from `subscript(index: Int, ...)` (before the `->`).
2. Parses `get(self)` or `get(let self)` block — synthesizes `let self` if no
   convention keyword written.
3. If present, parses `set(inout self, value: T)` block — `inout` is mandatory
   here; `T` is validated against the return type.
4. Produces a single `SubscriptDef` with `get_body` and optional `set_body`.

If the parser sees a bare block (shorthand form, no `get`/`set` keywords), it
produces a `SubscriptDef` with `get_body` populated and `set_body = None` (read-only).

## 3. What's in scope, what's deferred

### 3.0 In scope (v0.next — this proposal)

- Parser: `get`/`set` blocks inside `subscript` body
- HIR: unified `SubscriptDef` with `get_body`/`set_body`
- Typeck: resolve `base[i]` → `get` block, `base[i] = v` → `set` block
- Typeck: error on mutation of `self` inside `get` block
- Typeck: error on second `subscript(i: Int) -> T` on same type (duplicate)
- Lowering: emit `MethodCall subscript(inout self, i)` for reads,
  `MethodCall subscript_set(inout self, i, value)` for writes (same IR as today)
- IR: no new instructions — `MethodCall` with `let`/`inout` conventions
- VM: no change — the existing `inout` write-back handles `set` blocks
- Stdlib: updated `list.ax` subscript to new syntax
- Showcase: updated `place_assignment.ax` Grid to new syntax
- Remove: `SubscriptDef.is_setter`, the two-declaration model, synthesized `self` in lowerer

### 3.1 Deferred

| Item | Why deferred |
|---|---|
| Auto-detection of `let self` from usage (no need to write `let` in `get`) | Parser work: default convention when `self` has no prefix |
| `yield`-based projection | Coupled to Perceus + exclusivity (v1) |
| Multi-index conflict analysis (`xs[0]` vs `xs[1]` overlap) | Borrow-checking layer (v1) |
| Type subscripts (`static subscript`) | No use case in v0 |

## 4. Checklist — TDD order (red first, never weaken a test)

- [ ] **0. This doc.** Review and decide: adopt or reject.

### Layer 1 — Parser

- [ ] **1a. Parser: `get`/`set` as contextual keywords.** Add `Get`/`Set` to syntax
      kinds. Parse `get(self) { body }` and `set(inout self, value: T) { body }` inside
      `subscript_def`. The shorthand (no `get`/`set` → read-only) must still parse.
- [ ] **1b. Parser: reject duplicate blocks.** Error if two `get` or two `set`
      blocks appear in one `subscript` declaration.
- [ ] **1c. Parser tests.** Each parse shape has an inline snapshot test
      (the `axiom-parser/.../tests.rs` pattern). Include: read-only shorthand,
      full get+set, multi-param, error on duplicate `get`, error on `set` without
      `inout self`.

### Layer 2 — HIR

- [ ] **2a. New `SubscriptDef` structure.** Replace `is_setter: bool` + single body
      with `get_body` / `set_body: Option<SetBody>`. Remove the HIR lowerer's
      synthesized `self` — `get`/`set` blocks provide explicit `self`.
- [ ] **2b. AST → HIR lowering.** Map `get`/`set` AST blocks to the new HIR
      structure. Synthesize `let self` for `get` blocks that omit the convention
      keyword. Validate `set` block has `inout self` and `value: T`.
- [ ] **2c. HIR serialize.** Update the HIR serializer (used by snapshot tests) to
      emit the new structure. Regenerate all golden `.hir` files.
- [ ] **2d. Duplicate detection.** Add an invariant in `ImplDef` building that
      rejects two `SubscriptDef`s with the same index parameter count (preventing
      the silent-duplicate-subscript gap).

### Layer 3 — Typeck

- [ ] **3a. Unify resolver.** Replace `find_impl_subscript` / `find_impl_write_subscript`
      with a single `find_impl_subscript` that returns the `SubscriptDef` and the
      caller specifies read vs write context.
- [ ] **3b. Borrow-check `get` body.** Reject assignments to `self` or `self.*`
      inside a `get` block whose `self` convention is `let`.
- [ ] **3c. Validate `set` value type.** The `set` block's `value` parameter type
      must match the subscript's declared return type.
- [ ] **3d. Diagnostic: read-only subscript in write context.** If `base[i] = v` or
      `base[i] += v` targets a subscript with no `set` block, emit a clear
      diagnostic (replaces `NoWritableSubscript`).
- [ ] **3e. Diagnostic: duplicate subscript.** If an impl defines two subscripts with
      the same index parameter count, emit a diagnostic.

### Layer 4 — IR lowering

- [ ] **4a. Update `lower_subscript_def`.** Produce two IR functions per
      `SubscriptDef`: `Type::subscript` from `get_body`, `Type::subscript_set` from
      `set_body` (if present). The `inout self` in `set` flows through to the IR
      param convention.
- [ ] **4b. Lowering dispatch.** `lower_index_read` / `lower_index_write` dispatch
      to the same `MethodCall` names as today — no change needed.
- [ ] **4c. IR goldens.** Update `desugar_goldens/index_assign.ir` and add goldens
      for the read-only subscript case.

### Layer 5 — VM

- [ ] **5. No VM changes required.** The VM already handles `MethodCall` with
      `let`/`inout` self conventions. Existing `UnsupportedIndexBase` guard (H4)
      unchanged.

### Layer 6 — Stdlib & showcase

- [ ] **6a. Update `list.ax`.** Replace the two separate subscript declarations
      with a single `subscript(index: Int) -> T { get { self.buf[index] } set(inout self, value: T) { self.buf[index] = value } }`.
- [ ] **6b. Update `place_assignment.ax`.** Migrate the Grid subscript to the new
      syntax.
- [ ] **6c. HIR snapshot goldens.** Regenerate all HIR goldens that include the
      stdlib's `List` impl.

### Layer 7 — Test & guard

- [ ] **7a. Subscript-shape coverage matrix (H3 analogue).** Data-driven matrix:
      `{ shape: read-only | read+write } × { arity: 1 | 2 } × { op: read | = | += | -= | *= | /= | %= }`.
      Each cell has a real-output assertion. Drift guard: new shape or op without
      a row fails the build.
- [ ] **7b. Diagnostic snapshot tests.** One `.stderr` golden each for: duplicate
      subscript, write-on-read-only, `get` body mutates `self`, `set` body missing
      `inout`.
- [ ] **7c. No silent duplicate guard (H6).** Static assertion in typeck that an
      impl may contain at most one `SubscriptDef` per index-parameter count.
      Adding a second must fail the build with a diagnostic — never silently
      ignored.
- [ ] **7d. No synthesized `self` guard (H7).** Source-scan invariant: the HIR
      lowerer for subscripts must not contain logic that synthesizes a `self`
      parameter from a non-`self` AST node. `self` must appear in the AST
      `get(self)` / `set(inout self, ...)` blocks. (Prevents regressing to the v0
      invisible-mutability gap.)
- [ ] **7e. Migration invariant (H8).** After migration, no `SubscriptDef.is_setter`
      field must exist in the HIR. Enforce via `#[deny(dead_code)]`-style check
      or a compile-fail test that references the removed field.

### Layer 8 — Docs

- [ ] **8a. Update `DESIGN_SPEC.md` §4.4.** Replace the current two-form spec with
      the `get`/`set` model. Move the `yield`-based design to a "v1 projection"
      appendix or deferred note.
- [ ] **8b. Update `ir-design.md` §3.3.1.** The two-IR-function pattern
      (`subscript`/`subscript_set`) stays; note that both are now emitted from a
      single `SubscriptDef` with `get_body`/`set_body`.
- [ ] **8c. Update `vm-design.md` and `ir-design.md`.** No new instructions — note
      that the `UnsupportedIndexBase` guard still covers the non-HeapPtr path.

## 5. Harness & drift guards

Mirroring `docs/mutable-subscript-design.md` §7 and
`docs/lang-items-and-desugaring-design.md` §6.

### H3' — Subscript-shape coverage matrix — **Hard**

A data-driven matrix over `{ shape } × { arity } × { op }` where every cell
asserts real program output (`trace.output()`). Drift-guarded: adding a new
subscript shape (e.g. `write-only`) or a new operator without a covering row
fails the build.

### H6 — No silent duplicate subscript — **Hard**

An impl with two `subscript(i: Int) -> T` declarations (or any two subscripts
with the same index-param count) must produce a compile error. Never silently
pick one and ignore the other. This is a direct closure of the
`find_impl_subscript` → `.find()` silent-ignore gap.

### H7 — No synthesized `self` — **Hard**

The HIR lowerer for subscripts must not contain logic that creates a `self`
parameter where none exists in the AST. `self` must appear in the `get(self)` /
`set(inout self, ...)` blocks parsed from source. A source-scan test fails the
build if the lowerer synthesizes a `self` param from a non-`SelfParam` AST node
inside subscript handling.

### H8 — Removed field invariant — **Hard**

After migration, `SubscriptDef.is_setter` must not exist. A compile-fail test
referencing `SubscriptDef { is_setter: ... }` guarantees removal.

### What's mechanized vs judgment

| Guarantee | Mechanism | Strength |
|---|---|---|
| Read/write dispatch reaches correct block | Lowering + real-output e2e tests | **Hard** |
| `self` convention visible in source | H7: no-synthesized-self guard | **Hard** |
| No silent duplicate subscripts | H6: typeck diagnostic | **Hard** |
| Write on read-only subscript errors | Typeck diagnostic + `.stderr` golden | **Hard** |
| `get` body cannot mutate `self` | Typeck borrow-check + `.stderr` golden | **Hard** |
| Every (shape × arity × op) combo covered | H3' coverage matrix + drift guard | **Hard** |
| Old `is_setter` field removed | H8: compile-fail test | **Hard** |
| Semantics match DESIGN_SPEC intent | Review against spec | **Soft at the margin** |

## 6. Migration plan — every file that must change

The current subscript model touches 22 Rust source files, 4 `.ax` stdlib/showcase
files, and 3 golden snapshot directories. This section enumerates every file and
the migration action required — the implementation checklist references this section.

### 6.1 Files by migration order (dependency chain)

#### Stage A — Parser (lowest layer)

| File | Current state | Migration action |
|---|---|---|
| `crates/axiom-parser/src/syntax_kind.rs` | `SubscriptDef` node kind | Add `GetBlock`/`SetBlock` syntax kinds; remove `SelfParam` synthesis for subscript |
| `crates/axiom-parser/src/grammar/item.rs:353` | `subscript_def()` parses `param_list`, optional `-> Ty`, `block` | Rewrite to parse `get(self) { ... }` / `set(inout self, value: T) { ... }` blocks; shorthand (no get/set → read-only) must still parse |
| `crates/axiom-parser/src/ast/item.rs:369` | `SubscriptDef(SyntaxNode)` with `ret_type()`, `param_list()`, `body()` | Add `get_body()` / `set_body()` accessors |
| `crates/axiom-parser/src/ast/item_part.rs:564` | `impl_members.subscripts()` | No change (still returns `Vec<SubscriptDef>`) |
| `crates/axiom-parser/src/ast/tests.rs:20` | `SubscriptDef::can_cast(kind)` in AST node coverage | Update to include new block kinds |

#### Stage B — HIR (second layer)

| File | Current state | Migration action |
|---|---|---|
| `crates/axiom-hir/src/hir/items.rs:126` | `SubscriptDef { id, params, return_type, body, is_setter }` | Replace with `{ id, index_params, return_type, get_body, set_body: Option<SetBody>, has_explicit_get }`. Remove `is_setter`. |
| `crates/axiom-hir/src/lower/item.rs:440` | `lower_subscript_def()` synthesizes `self` param, sets `is_setter` | Rewrite: parse `get` block → synthesize `let self`; parse `set` block → validate `inout self`, synthesize `value` param. No more `is_setter`. |
| `crates/axiom-hir/src/serialize/mod.rs` | Serializes `SubscriptDef` including `is_setter` | Update to serialize `get_body`/`set_body` structure |
| `crates/axiom-hir/src/resolve/item.rs` | Resolves names in subscript body | Update to resolve names in both `get_body` and `set_body` blocks |
| `crates/axiom-hir/src/lang.rs` | `SUBSCRIPT_SET = "subscript_set"`, `subscript_set_fn()` | No change — IR-level names unchanged |
| `crates/axiom-hir/src/lib.rs` | Re-exports `SubscriptDef` | Update exports |
| `crates/axiom-hir/tests/invariants.rs` | References `SubscriptDef` in variant coverage | Update to cover new structure |
| `crates/axiom-hir/tests/fuzz.rs` | Generates `SubscriptDef` with `is_setter` | Update fuzz generator |
| `crates/axiom-hir/tests/fixtures/modules/*/main.hir` | HIR golden snapshots containing `SubscriptDef` | Regenerate with `UPDATE_SNAPSHOTS=1` |

#### Stage C — Typeck (third layer)

| File | Current state | Migration action |
|---|---|---|
| `crates/axiom-typeck/src/typeck/methods.rs:236` | `find_impl_subscript()` — finds first `!is_setter`. `find_impl_write_subscript()` — finds first `is_setter` | Unify into single `find_impl_subscript() -> &SubscriptDef`. Caller specifies read/write via a `SubscriptMode` enum `{ Read, Write }`. Add duplicate-detection: if an impl has two `SubscriptDef`s with the same index-param count, emit diagnostic. |
| `crates/axiom-typeck/src/typeck/mod.rs` | Imports `ImplInfo`, passes subscripts around | Update `ImplInfo.subscripts` field type; update typeck layer to consume new HIR structure |
| `crates/axiom-typeck/src/coverage.rs` | References `SubscriptDef` in variant coverage | Update to cover new structure |
| `crates/axiom-typeck/src/serialize/mod.rs` | Serializes typeck info including subscripts | Update serialization |
| `crates/axiom-typeck/tests/diagnostics.rs` | Test for `NoWritableSubscript` diagnostic | Update to test new diagnostic. Add tests for: duplicate subscript, write-on-read-only, `get` body mutates `self` |
| `crates/axiom-typeck/tests/builtin_traits.rs` | May reference subscript through type resolution | Verify and update if needed |

#### Stage D — IR lowering (fourth layer)

| File | Current state | Migration action |
|---|---|---|
| `crates/axiom-ir/src/lower/item.rs` | `lower_subscript_def()` emits `subscript_fn`/`subscript_set_fn` based on `is_setter` | Update to emit from `get_body` → `subscript_fn`, `set_body` → `subscript_set_fn`. Same IR instruction names — no VM change. |
| `crates/axiom-ir/src/lower/expr.rs:233` | `lower_index_read` / `lower_index_write` dispatch to `subscript_fn`/`subscript_set_fn` | No change — same IR function names |
| `crates/axiom-ir/src/lower/assign.rs` | `lower_assign_index` calls `lower_index_read`/`lower_index_write` | No change |
| `crates/axiom-ir/tests/desugar_coverage.rs` | `SugarSpec` for `index_assign` with expected calls | Update expected calls if IR output changes; regenerate `desugar_goldens/index_assign.ir` |
| `crates/axiom-ir/tests/desugar_goldens/index_assign.ir` | Golden IR snapshot | Regenerate with `UPDATE_SNAPSHOTS=1` |

#### Stage E — VM (fifth layer)

| File | Current state | Migration action |
|---|---|---|
| `crates/axiom-vm/src/exec/instr.rs` | `IndexSet`/`Index` hard-error on non-`HeapPtr` | No change — `UnsupportedIndexBase` guard stays |
| All VM test files (*.rs) | Various test `.ax` programs define subscripts inline | Update `.ax` source strings in tests to new `get`/`set` syntax |

#### Stage F — Stdlib (.ax source)

| File | Current state | Migration action |
|---|---|---|
| `stdlib/std/collections/list.ax:73` | Read: `subscript(index: Int) -> T { self.buf[index] }` | → `subscript(index: Int) -> T { get { self.buf[index] } set(inout self, value: T) { self.buf[index] = value } }` |
| `stdlib/std/collections/list.ax:82` | Write: `subscript(index: Int, value: T) { self.buf[index] = value }` | Removed — merged into single declaration above |
| `stdlib/std/collections/map.ax:176` | Read-only: `subscript(key: K) -> V { ... }` | → `subscript(key: K) -> V { get(self) { ... } }` (read-only — no `set`) |
| `showcase/showcase.ax` | Uses List subscript reads only | No syntax change needed (read syntax at call site unchanged) |
| `showcase/place_assignment.ax` | Grid uses two subscript declarations | → Single `subscript(i: Int) -> Int { get(self) { self.buf[i] } set(inout self, value: Int) { self.buf[i] = value } }` |

#### Stage G — E2e test programs (.ax strings in Rust)

| File | Estimated inline subscript programs | Migration action |
|---|---|---|
| `crates/axiom-vm/tests/mutable_subscript_e2e.rs` | ~5 programs with two subscript declarations | Rewrite each to single `get`/`set` syntax |
| `crates/axiom-vm/tests/place_assign_matrix.rs` | ~1 program template (UserStruct branch) | Rewrite template |
| `crates/axiom-vm/tests/place_assign_e2e.rs` | Programs may reference subscripts | Verify; update if needed |
| `crates/axiom-vm/tests/subscript_e2e.rs` | Inline subscript tests | Rewrite to new syntax |
| `crates/axiom-vm/tests/list_e2e.rs` | Uses list subscript via stdlib | No change (stdlib migration handles it) |
| `crates/axiom-vm/tests/output_assertion_guard.rs` | Scans for `t.format()` | No change |
| `crates/axiom-vm/tests/invariants.rs` | Variant coverage | No change |

### 6.2 Migration rollback trap

**The two-declaration syntax must be deprecated with a clear diagnostic before removal.**
If we silently break `list.ax` syntax, `forge run showcase/showcase.ax` fails with an
inscrutable parse error. The parser must detect the old form (`subscript(index, value)`)
and emit: *"This form of subscript is deprecated; use `get`/`set` inside a single
`subscript(index: Int) -> T { ... }` declaration."*

This diagnostic lives for exactly one commit cycle (the migration commit itself) and
is removed in the same commit that updates the stdlib and all downstream `.ax` files.

### 6.3 Migration verification gate

After all files are migrated, the following must pass:

```bash
# 1. All existing stdlib/showcase programs still run and produce identical output
cargo run -p axiom-cli -- run showcase/showcase.ax
cargo run -p axiom-cli -- run showcase/place_assignment.ax

# 2. Full test suite (fmt + clippy + test)
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test

# 3. HIR goldens regenerated with UPDATE_SNAPSHOTS=1
UPDATE_SNAPSHOTS=1 cargo test -p axiom-hir --test multi_file_golden

# 4. IR desugar goldens regenerated
UPDATE_SNAPSHOTS=1 cargo test -p axiom-ir --test desugar_coverage

# 5. Trace goldens unchanged (no VM change)
UPDATE_SNAPSHOTS=1 cargo test -p axiom-vm --test golden

# 6. All new H3'/H6/H7/H8 guard tests pass
cargo test -p axiom-typeck --test diagnostics
cargo test -p axiom-vm --test invariants
cargo test -p axiom-vm --test output_assertion_guard
```

## 7. Comparison with alternatives

| | Current v0 (two decls) | Spec end-state (yield) | **Swift get/set (this proposal)** |
|---|---|---|---|
| One declaration | ✗ | ✓ | ✓ |
| `inout` visible | ✗ (synthesized) | ✓ (explicit) | ✓ (explicit) |
| No write-in-read hole | ✓ | ✓ (yield semantics) | ✓ (structural split) |
| No silent duplicates | ✗ (`.find` picks first) | ✗ (same risk) | ✓ (typeck guards) |
| No new VM instruction | ✓ | ✗ (projection open/resume) | ✓ |
| No new semantic concept | ✓ | ✗ (`yield` suspend) | ✓ (reuses conventions) |
| Settler in v0.next | — | ✗ (needs Perceus) | ✓ |
| Migration cost | — | High (parser + typeck + VM) | Medium (parser + HIR) |

## 8. Decisions vs open questions

### 8.0 Decided (if this proposal is adopted)

- Reject the `yield`-based end-state for v0/v1; use `get`/`set` blocks instead.
- `set` block requires explicit `inout self` and explicit `value: T`.
- `get` block defaults `self` to `let` (no keyword required, but allowed).
- At most one subscript per index-parameter count per type.

### 8.1 Open questions

| # | Question | Lean |
|---|---|---|
| O-SS1 | Should `get(self)` default `self` to `let` without the keyword, or require `get(let self)`? | Default — same as `fn sound(self)` today. Write `let` only for emphasis. |
| O-SS2 | What is the diagnostic wording for "write on read-only subscript"? | "type `T` has a read-only subscript; cannot assign to `xs[i]`" |
| O-SS3 | Does the `set` block's `value: T` inherit the return type automatically, or is explicit annotation required? | Auto-inherit; repeated annotation is a parse error. |
| O-SS4 | Should we support a `set`-only subscript (write-only, never read)? | Defer — no use case in v0. Can add later by making `get_body` optional. |

## 9. Cross-references

- `DESIGN_SPEC.md` §4.4 (subscripts as in-place lenses — will be updated to
  `get`/`set` model).
- `docs/mutable-subscript-design.md` (the v0 bug fix that this proposal replaces).
- `stdlib/std/collections/list.ax` (the target for the first migration).
- `showcase/place_assignment.ax` (the showcase program to update).
- `crates/axiom-hir/src/hir/items.rs:126` (`SubscriptDef` — will be restructured).
- `crates/axiom-hir/src/lower/item.rs:440` (`lower_subscript_def` — will lose
  synthesized `self`).
- `crates/axiom-typeck/src/typeck/methods.rs:236` (`find_impl_subscript`/`find_impl_write_subscript`
  — will be unified).
- `crates/axiom-ir/src/lower/expr.rs:233` (`lower_index_read`/`lower_index_write`
  — unchanged in logic, but source of the two-IR-function names now traced to one HIR node).
