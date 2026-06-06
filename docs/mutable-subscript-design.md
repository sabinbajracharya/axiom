# Mutable Subscript / Indexed-Place Assignment — Design Plan

> **Status: [Fixed; implemented; all guards mechanized].** `base[i] = v` and
> `base[i] op= v` on a library collection (`List<T>`) now work correctly. The
> v0 interim fix uses setter-desugar (§4.2); the full `inout` projection (§4.1)
> lands with the memory model in v1. All H1–H4 drift guards are in place.

## 0. The concern this answers

Axiom promises `xs[i] = v` and `xs[i] += v` as the **one obvious way** to mutate an element
in place (`DESIGN_SPEC.md` §2.7 operators, §4.4 subscripts-as-lenses). For the raw `[T]`
heap-buffer floor this works. **For every library collection (`List`, and any user struct
with a `subscript`) it does not** — the write is dropped or crashes. Worse, the existing
tests are **green**, because they assert against the execution *trace text* rather than the
program's real output, so a silent no-op slips through. This is precisely the failure mode
the lang-items doc warns about (`lang-items-and-desugaring-design.md` §6.4, "the prefix
lesson in `list_e2e.rs`").

## 1. Reproduction & evidence (real output, not trace substrings)

Each program below was run through the full pipeline (`check_modules → monomorphize →
lower → Vm`) asserting on the **`output` trace entries only** (`entry.fn_name == "output"`),
i.e. what the program actually printed:

| Program | Expected | **Actual** | Verdict |
|---|---|---|---|
| `var xs = [1,2,3]; xs[0] = 9; print(xs[0])` | `9` | **`1`** | silent no-op — element never written |
| `var x = [10,20,30]; x[0] = x[2]; print(x[0])` | `30` | **`10`** | silent no-op |
| `var a = [1,2,3]; a[1] += 23; print(a[1])` | `25` | **crash** `BranchTypeMismatch { got: "Unit + Int" }` | compound read-back returns `Unit` |
| `xs[1]` as a **read** expression | element | element ✓ | reads are fine (dispatch to `List::subscript`) |
| `xs.push(v)` | works ✓ | works ✓ | push writes through `self.buf` (a real `[T]`) |

The asymmetry is the tell: **reads and `push` work; direct indexed-place writes do not.**

## 1.1 Why `push` works but `xs[i] = v` does not

`List<T>` is `struct List { buf: [T], count, cap }` (`stdlib/std/collections/list.ax`).
`push` does `self.buf[self.count] = element` — an assignment to **`self.buf`, which is a `[T]`
heap buffer** (`Value::HeapPtr`), so the primitive `IndexSet` writes correctly. But
`xs[i] = v` assigns to **`xs` itself, a `List` struct** (`Value::Struct`), and the primitive
`IndexSet` only knows how to write `HeapPtr`. So it silently falls through.

## 2. Root cause — three layers, all real

### 2.1 Surface / stdlib — no mutable subscript exists
`List` defines exactly **one, read-only** subscript (`list.ax:73`):

```rust
subscript(index: Int) -> T { self.buf[index] }   // read projection only
```

There is no mutable/`inout` form. `DESIGN_SPEC.md` §4.4 calls for the write half:

```rust
subscript(inout self, i: Int) -> T { yield inout self.buf[i] }   // [Decided] — not implemented
```

`SubscriptDef` (`axiom-hir/src/hir/items.rs:121`) carries only `{ params, return_type, body }`
— no read/write distinction, no `inout self` mutability marker, no `yield`. `yield` is a
lexer keyword (`axiom-lexer/.../token.rs`) with **no IR/VM execution machinery** behind it.

### 2.2 IR lowering — the write path never dispatches to a subscript
The **read** path is correct: `lower_index` (`axiom-ir/src/lower/expr.rs:218`) branches on
`is_heap_buffer` and, for a `List`, emits `MethodCall List::subscript(...)`
(`expr.rs:235`). The **write** path has no such branch: `lower_assign_index`
(`axiom-ir/src/lower/assign.rs:96`) **always** emits the primitive `IrInstr::IndexSet`
regardless of base type, and for compound ops reads the old value via the primitive
`IrInstr::Index` (`assign.rs:104`). Neither is subscript-aware.

### 2.3 VM — primitives silently no-op / return `Unit` on a struct
- `IrInstr::IndexSet` (`axiom-vm/src/exec/instr.rs:308`) writes **only** `if let Value::HeapPtr(addr) = base`; a `Value::Struct` falls through and **does nothing** (`instr.rs:317`).
- `IrInstr::Index` (`instr.rs:161`) returns `Value::Unit` for any non-`HeapPtr` base (`instr.rs:168`). That `Unit` is what feeds the compound `Unit + Int` crash.

This **silent fall-through to `Unit`/no-op is the root enabler** — it converts a missing
feature into a quiet wrong answer instead of a loud error (cf. §7, guard H4).

## 3. Why this is spec-relevant (not just a backend gap)

`DESIGN_SPEC.md` §4.4 ("Subscripts as in-place lenses", **[Decided]**) makes the mutable
subscript the *defining mechanism* of the language's memory model: `xs[i] += 10` "desugars
to: take inout projection of element 1, mutate in place, resume" (§4.4 line 293). The two
projection rules were exercised in Spike 0b (§4.4 spike note; `spike-0-findings.md`). So the
**design** is settled; the **v0 implementation** simply never built the write half for
library types — only the raw-buffer floor and the read projection.

## 4. The fix — interim vs end-state (mirroring the lang-items doc's split)

### 4.1 [Deferred, end-state] Full §4.4 `inout` projection
Implement `yield inout` end-to-end: HIR gains an `inout self` subscript + `yield`; IR lowers
a *projection open → mutate → resume* sequence; the VM executes a borrow lend/resume with no
escaping pointer. This is the spec-true target and the right home **once the real memory
model lands (v1, Perceus + exclusivity)** — it is heavy and entangled with borrow checking.

### 4.2 [Decided, interim] v0 setter-desugar (recommended to unblock now)
Until the projection machinery exists, lower an indexed-place **write** the same way the
**read** already works — as a method call on the receiver — to a **mutable subscript setter**:

- **Stdlib:** add the write form to `List` (and document the convention):
  ```rust
  subscript(inout self, index: Int, value: T) { self.buf[index] = value }   // setter body
  ```
  The body writes through `self.buf` (a real `[T]`), which already works.
- **Lowering (`lower_assign_index`):** for a non-`HeapBuffer` base, stop emitting the raw
  `IndexSet`. Instead emit `MethodCall Type::subscript_set(inout base, index, value)`. For
  compound `op=`, read the old element with the **existing read-subscript path** (reuse
  `lower_index`'s dispatch, *not* the raw `IrInstr::Index`), apply the `BinOp`, then call the
  setter. Raw `[T]` bases keep the fast primitive `IndexSet`/`Index` path unchanged.
- **VM:** no new instruction needed — it's an ordinary `inout self` method call, and
  `inout`-receiver write-back already works (that is how `push` mutates `self`).

This reuses the name-keyed dispatch the VM already uses for reads, costs no new memory-model
machinery, and is a behaviour-preserving precursor to §4.1 (the setter is exactly what the
`inout` projection will desugar to internally).

### 4.3 Decision
**Adopt §4.2 for v0; converge on §4.1 when the memory model lands.** Record the surface
syntax of the write form (a dedicated `subscript(inout self, index, value)` setter vs. a
single `inout`-projection subscript) as **open question O-MS1** below — the *interim* picks
the setter; the *end-state* picks the projection.

## 5. Step-by-step checklist (TDD — red first, never weaken a test)

> Build order mirrors `lang-items-and-desugaring-design.md` §7: lock the failing behaviour
> with **real-output** tests first, then implement until green.

- [x] **1. Red tests (real output).** Add e2e tests asserting the *runtime* result (via the
      `output`-only helper, §7 H1), each using a value **not present as a literal** in the
      source so a no-op cannot pass (§7 H2): `xs[0] = compute()`, `x[i] = x[j]`,
      `a[i] += n`, `a[i] -= n`, on `List<Int>` and on a user struct with a subscript. They
      must **fail** against today's code (no-op / `Unit + Int`).
- [x] **2. VM fall-through guard (H4).** Make `IndexSet`/`Index` on a non-`HeapPtr` base a
      hard `VmError` (e.g. `UnsupportedIndexBase`) instead of no-op/`Unit`. Re-run §1 repros:
      the silent cases now error loudly. (This alone converts the bug from silent to visible.)
- [x] **3. Stdlib setter.** Add the `List` write subscript (§4.2). Decide & document the
      surface syntax (O-MS1). Update `stdlib/std/collections/list.ax` + its README.
- [x] **4. Subscript-set resolution/typeck.** Allow a type to carry a **read** and a
      **write** subscript; resolve `base[i] = v` / `op=` to the write form in
      `infer_index`/assignment checking (`axiom-typeck/.../typeck/methods.rs:437`,
      `find_impl_subscript`). Emit a clear diagnostic when a base is assigned-into but has no
      writable subscript.
- [x] **5. Lowering.** Rewrite `lower_assign_index` (`axiom-ir/src/lower/assign.rs:96`): raw
      `[T]` → keep `IndexSet`; library type → `MethodCall Type::subscript_set(inout base, i,
      v)`. Compound: old value via the read-subscript dispatch (factor a shared
      `lower_index_read` helper out of `lower_index`), then `BinOp`, then setter.
- [x] **6. Goldens.** Pin the desugared IR for one `=` and one `+=` case (the
      `desugar_goldens` pattern), showing the setter call chain — not a raw `IndexSet` on a
      struct.
- [x] **7. Green + coverage.** All §1 repros return correct values. Add the place-assignment
      coverage matrix (§7 H3) and make it pass. `cargo fmt && clippy -D warnings && test`.
- [x] **8. Fix the misleading legacy tests.** Convert `test_list_index_assignment_runs`
      (`place_assign_e2e.rs`) and any `list_e2e`/`map_e2e`/`subscript_e2e` value assertions to
      the real-output helper; delete trace-substring value checks. (They currently pass for
      the wrong reason — see §6.)
- [x] **9. Spec/docs sync.** Note in `DESIGN_SPEC.md` §4.4 that the write form is
      implemented via the interim setter in v0; update `vm-design.md`/`ir-design.md` for the
      new lowering and the `UnsupportedIndexBase` error.

## 6. How we missed it (the testing gap — fix the method, not just the bug)

`run_output` in the e2e suites returns `vm.take_trace().format()` — the **full instruction
trace as text** — and tests assert with `out.contains("9")`. But the trace contains
`Const(Int(9))` from *constructing* the value, so the substring matches **whether or not the
write happened**. Three things compounded:

1. **Assertions matched the trace, not the output.** The real printed text lives only in
   trace entries with `fn_name == "output"` (`axiom-vm/.../exec/builtins.rs:210`); no test
   filtered to those.
2. **Literals leaked into the trace.** Asserting a value that *also appears as a source
   literal* can never distinguish "written" from "constructed".
3. **The VM hid the failure.** `IndexSet`/`Index` silently degrade to no-op/`Unit` instead
   of erroring, so nothing surfaced at runtime either.

The lang-items doc already named this exact trap (§6.4, §6.5 "Literal yields a real stdlib
struct… structural assert + capacity golden"). We documented the lesson but did not
mechanize it for *place assignment*.

## 7. Harness & drift guards (so this class can't return)

> **All mechanized.** Each guard below is implemented in the test suite as of the
> v0 interim fix. The source of truth for test file locations: `place_assign_matrix.rs`
> (H3), `output_assertion_guard.rs` (H1), and `crates/axiom-vm/src/exec/instr.rs` (H4).

Mirroring `lexer-testing.md` §4/§9, `collection-type-design.md` §8, and
`lang-items-and-desugaring-design.md` §6 — **mechanize the "can't silently drift" guard.**

### H1 — Real-output assertion helper — **Hard**
A single shared `run_program(src) -> String` returning **only** concatenated `output`
entries (`entry.fn_name == "output"`). All behavioural e2e assertions go through it. Add a
source-scan `#[test]` (the lexer §5.2 pattern) that **fails the build** if a `*_e2e.rs`
asserts a value substring against a raw `format()`/full-trace string instead of `run_program`.

### H2 — "Mutation actually happened" invariant — **Hard**
Every place-assignment test must assert an observed value that is **(a)** different from the
pre-assignment value **and (b)** not present verbatim as a literal in the source (e.g. assign
a runtime-computed value). This makes both a silent no-op *and* a trace-substring false-pass
impossible.

### H3 — Place-assignment coverage matrix — **Hard**
A data-driven matrix, drift-guarded like the `IrInstr` variant-coverage tests
(`axiom-vm/tests/invariants.rs`) and `desugar_coverage.rs`:
`{ target: name | field | index } × { op: = | += | -= | *= | /= | %= } × { base: [T] HeapBuffer | List | user-struct-with-subscript }`.
Each cell has a real-output assertion. **Adding a new `AssignTarget` variant or a new
indexable base kind without a row fails the test** — the same "a case I never imagined
slipped through" guard the lang-items doc's §6.3 uses.

### H4 — No silent fall-through in the VM — **Hard**
`IndexSet`/`Index` (and any place primitive) on an unsupported base kind must be a typed
`VmError`, never a no-op or `Value::Unit`. This is the direct analogue of the lang-items
§6.4 "no compiler-native value" invariant: a missing capability must fail loudly. *This is
the single guard that would have caught the original bug on day one.*

### H5 — What's mechanized vs judgment

| Guarantee | Mechanism | Strength |
|---|---|---|
| Behavioural tests assert real program output | `run_program` helper + source-scan ban on trace-substring value asserts | **Hard** |
| A no-op write cannot pass a test | H2 distinct-value rule | **Hard** |
| Every (target × op × base) combo is covered | H3 coverage matrix + drift guard | **Hard** |
| Unsupported index base fails loudly | H4 `VmError` (no silent `Unit`/no-op) | **Hard** |
| Desugared write is the setter call, not a raw struct `IndexSet` | IR golden | **Hard** |
| Indexed write matches §4.4 semantics end-to-end | review against `DESIGN_SPEC` §4.4 | **Soft at the margin** |

## 8. Decisions vs open questions

### 8.1 Decided
- **The bug is real and the read/`push` paths are fine; only indexed-place *writes* on
  library types are unimplemented** (§1, §2).
- **v0 fix = setter-desugar (§4.2); end-state = full `inout` projection (§4.1).**
- **The VM must not silently fall through on unsupported index bases** (H4).
- **Behavioural tests assert real output, not trace text** (H1/H2).

### 8.2 Open questions

| # | Question | Lean |
|---|---|---|
| O-MS1 | Surface syntax of the write subscript — a dedicated `subscript(inout self, index, value)` setter, or one `inout`-projection subscript (`yield inout …`) that serves both read & write? | **resolved for v0: setter** (implemented); end-state: **projection** (§4.4) |
| O-MS2 | Should compound `a[i] += v` evaluate the index **once** (bind to a temp) to avoid double-evaluating an effectful index expression? | **yes** — eval index once (matches §4.4 "eagerly-evaluated operands") |
| O-MS3 | Do `OrderedMap`/`Set`/user structs reuse the same setter convention, or is it `List`-specific for v0? | shared convention, `List` first |

## 9. Cross-references

- `DESIGN_SPEC.md` §4.4 (subscripts as in-place lenses, the `inout` projection), §2.7
  (operators incl. `+=` and `[]`), §15 (open questions), §14 (roadmap — memory model is v1).
- `lang-items-and-desugaring-design.md` §6 (harness templates), §6.4 (the trace-substring
  trap), §5 (`base[i]` subscript row / O3).
- `stdlib/std/collections/list.ax` (the read-only subscript + `push`), `vm-design.md`,
  `ir-design.md` (`IndexSet`/`Index` and assignment lowering).
- `axiom-vm/tests/invariants.rs`, `axiom-ir/tests/desugar_coverage.rs`,
  `axiom-vm/tests/place_assign_e2e.rs` (the test that currently passes for the wrong reason).
- `spike-0-findings.md` (subscript × exclusivity × loops, Spike 0b).
