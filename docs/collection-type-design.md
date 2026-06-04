# Collection Type Design — Path A (Library Types)

> **Status:** authoritative for the collection type design. Binding before code is written.
> **Decisions baked in:** library-type collections on a raw memory primitive, flat/inline element
> storage, container-level Perceus refcount, `yield`-based subscript projections, monomorphized
> generics with trait bounds.
> **Prerequisites:** generics (§3.6), traits (§3.5), `Deinit` trait, `DynamicBuffer` primitive,
> subscript declarations (§4.4). **This doc is implementable once those exist.**
> **Companion docs:** [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §3.2, §3.5, §3.6, §4.4–§4.9,
> [`collection-type-research.md`](collection-type-research.md) (the research that led here),
> [`spike-0-findings.md`](spike-0-findings.md) (subscript × exclusivity validation),
> [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

---

## 0. The concern this answers

The design spec (§3.2) lists `List<T>`, `Map<K,V>`, `Set<T>`, `OrderedMap<K,V>`, and
`OrderedSet<T>` as built-in collections. But it doesn't say what they *are* in the type system,
how they store elements, how they interact with Perceus refcounting, or how they're tested.
The audit (item #3) flags this as an OPEN must-fix.

The fear: **collections are mentioned but undeclared**, which means the type checker, IR
generator, and memory model all have a hole where collection semantics should be. If we
don't pin the design now, we'll bake assumptions into the compiler that contradict the final
answer.

Two ideas carry the weight: collections are **library types** backed by a compiler-provided
**raw memory primitive** (`DynamicBuffer`), and the compiler never mentions `List`, `Map`, or
`Set` by name — it only knows about `DynamicBuffer`, generics, traits, subscripts, and Perceus.

---

## 1. The design, stated plainly

### 1.1 What the compiler provides (the primitive layer)

The compiler provides exactly one collection-relevant primitive:

```
DynamicBuffer<Header, Element>
```

- A heap-allocated contiguous buffer with a header and `capacity` element slots.
- Layout: `[refcount | Header | Element[0] | Element[1] | ... | Element[capacity-1]]`
- The compiler knows how to: allocate, initialize elements, deinitialize elements, deallocate.
- `DynamicBuffer` is **not a user-facing type** — it's the IR-level primitive that library
  collection types build on.

Everything else — `List<T>`, `Map<K,V>`, `Set<T>` — is library code.

### 1.2 What the standard library provides (the collection layer)

```axiom
// In the standard library, not the compiler
struct List<T: Deinit> {
    buffer: DynamicBuffer<Int, T>   // header = count (Int)
}

impl<T: Deinit> List<T> {
    // Construction
    fn new() -> List<T>
    fn from_array(sink elements: Array<T>) -> List<T>

    // Access
    fn count(let self) -> Int
    fn is_empty(let self) -> Bool
    fn capacity(let self) -> Int

    // Mutation
    fn push(inout self, sink element: T)
    fn pop(inout self) -> Option<T>
    fn reserve(inout self, additional: Int)

    // Subscript projections (§4.4)
    subscript(let self, i: Int) -> T {
        yield self.buffer[i]
    }
    subscript(inout self, i: Int) -> T {
        yield inout self.buffer[i]
    }
}

// Map and Set — also library types, require Hashable + Equatable
struct Map<K: Hashable + Equatable, V: Deinit> { ... }
struct Set<T: Hashable + Equatable> { ... }
struct OrderedMap<K: Equatable + Ord, V: Deinit> { ... }
struct OrderedSet<T: Equatable + Ord> { ... }
```

### 1.3 What this means for the compiler

The compiler is **collection-agnostic**. It handles:
- Generic structs (monomorphization)
- Trait bounds (constraint checking)
- `DynamicBuffer` allocation/deallocation/element lifecycle
- Subscript declarations with `yield`
- Perceus refcounting on heap values (including `DynamicBuffer`)

It never mentions `List`, `Map`, or `Set` by name. No `Ty::List`. No special-casing.

---

## 2. Type system representation

### 2.1 How collections appear in `Ty`

Collections are **nominal types** — they appear as `Ty::Nominal(def_id, args)` where
`def_id` points to the library struct definition and `args` are the concrete type arguments.

```
List<Int>    → Ty::Nominal(def_id_for_List, [Ty::Int])
Map<String, Int> → Ty::Nominal(def_id_for_Map, [Ty::String, Ty::Int])
```

There is no `Ty::List`, `Ty::Map`, or `Ty::Set`. The compiler treats them exactly like
any other generic struct.

### 2.2 The `Deinit` trait

Collections need to know how to destroy their elements. This requires a `Deinit` trait:

```axiom
trait Deinit {
    fn drop(inout self)   // called when the value leaves scope
}
```

- Every type implements `Deinit` automatically (trivial drop for value types, recursive
  drop for structs with refcounted fields).
- The compiler generates `Deinit` impls for all types — it's not opt-in.
- `List<T: Deinit>` means "T must be destructible" — which every type is, so the bound
  is satisfied universally. The bound exists to document the requirement and to enable
  user-defined containers that need it.

### 2.3 Trait bounds for `Map` and `Set`

```axiom
trait Hashable {
    fn hash(let self) -> Int
}
trait Equatable {
    fn eq(let self, let other: Self) -> Bool
}
```

- `Map<K: Hashable + Equatable, V: Deinit>` — keys must be hashable and equatable.
- `Set<T: Hashable + Equatable>` — elements must be hashable and equatable.
- `OrderedMap<K: Ord, V: Deinit>` — keys must be orderable (uses comparison, not hash).
- `OrderedSet<T: Ord>` — elements must be orderable.

---

## 3. Memory layout

### 3.1 The `List<T>` struct (stack/inline)

```
┌──────────────────┐
│ buffer: *T       │ ──── pointer to heap allocation (or null if empty)
│ len: usize       │     number of elements currently stored
│ cap: usize       │     number of element slots allocated
└──────────────────┘
  24 bytes on 64-bit (3 words)
```

The `List<T>` struct itself is always stack-allocated (or inline in a containing struct).
It is 3 words regardless of `T`'s size — identical to Rust's `Vec<T>` layout.

### 3.2 The heap allocation (when non-empty)

```
┌──────────────────┐
│ refcount: usize  │     Perceus-managed reference count
│ count: usize     │     same as len (redundant with struct, but DynamicBuffer needs it)
│ cap: usize       │     same as cap (redundant with struct, but DynamicBuffer needs it)
│ T[0]             │     element 0, stored inline (not boxed)
│ T[1]             │     element 1
│ ...              │
│ T[cap-1]         │     last allocated slot
└──────────────────┘
```

Elements are stored **flat/inline** — no pointer indirection per element. `List<Int>` stores
raw `Int` values contiguously. `List<String>` stores `String` structs contiguously (each
`String` is itself a small struct with a pointer to its heap buffer).

### 3.3 When Perceus proves unique ownership

When the compiler proves the `List<T>` has refcount 1 (unique ownership), the refcount
operations are elided entirely. The layout compiles to:

```
┌──────────────────┐
│ count: usize     │
│ cap: usize       │
│ T[0] ... T[cap-1]│
└──────────────────┘
```

This is identical to Rust's `Vec<T>` — zero overhead. The refcount header only exists
when the list is actually shared.

### 3.4 Empty list

An empty `List<T>` has `buffer: null, len: 0, cap: 0`. No heap allocation. Zero cost.

---

## 4. Element lifecycle

### 4.1 Initialization (`push`, `from_array`)

When an element is pushed, the `DynamicBuffer` initializes it in-place at the next slot:

```
push(element):
  if len == cap → reallocate (double capacity)
  initialize T[len] = element   // in-place init, not memcpy
  len += 1
```

The element is **moved** into the buffer (sink convention). The caller loses ownership.

### 4.2 Destruction (`drop`, `pop`)

When a `List<T>` is dropped:
1. Walk elements `0..len` and call `Deinit::drop` on each.
2. Deallocate the heap buffer.

When an element is popped:
1. Move `T[len-1]` out of the buffer (the buffer no longer owns it).
2. Mark the slot as uninitialized (the buffer doesn't drop it).
3. `len -= 1`.
4. Return the moved element as an `Option<T>::Some(value)`.

### 4.3 Destruction order

Elements are destroyed in **forward order** (0, 1, 2, ..., len-1). This is deterministic
and matches the spec's reverse-declaration-order rule for scope destruction (§4.9) at the
element level — the first element was declared first, so it's destroyed first.

> **Design note:** Rust drops Vec elements in forward order too. Forward is the natural
> choice for a contiguous buffer.

### 4.4 Reuse analysis / FBIP (§4.6)

When the compiler sees that a `List<T>` has refcount 1, operations like `map`, `filter`,
and `append` compile to in-place mutation:

```axiom
var xs = [1, 2, 3]
val ys = xs.map(|x| x * 2)   // xs has refcount 1 after this line
                               // → Perceus reuses xs's buffer for ys
                               // → no allocation, in-place transform
```

When the list is shared (refcount > 1), the operation allocates a new buffer. This is
Perceus's core value proposition — the programmer writes functional code, the compiler
generates imperative code when it can.

---

## 5. Subscript projections (§4.4)

### 5.1 Read projection

```axiom
subscript(let self, i: Int) -> T {
    yield self.buffer[i]
}
```

- Bounds-checked at runtime (panic on out-of-bounds).
- Returns a **temporary borrow** of the element — the caller can read it but not store
  a reference to it (no escaping references in MVS).
- The borrow lasts for one operation with eagerly-evaluated operands (Spike 0 rule #1).

### 5.2 Mutable projection

```axiom
subscript(inout self, i: Int) -> T {
    yield inout self.buffer[i]
}
```

- Bounds-checked at runtime.
- Returns a temporary `inout` borrow — the caller can mutate the element in-place.
- The subscript **suspends**, lends the element, and **resumes** when the caller is done.
- `xs[1] += 10` desugars to: take inout projection of element 1, add 10, resume.

### 5.3 Disjoint storage rule (Spike 0 rule #2)

Builtin collection indexing is treated as **disjoint storage**:
- `xs[0]` and `xs[1]` don't conflict — distinct elements may be mutated simultaneously.
- `xs[i]` and `xs[j]` conflict — can't prove `i != j` at compile time.
- This is already validated by Spike 0 (23/23 scenarios passed).

---

## 6. Literal syntax

### 6.1 List literals

```axiom
val xs: List<Int> = [1, 2, 3]       // desugars to List::from_array([1, 2, 3])
val ys = [1, 2, 3]                   // type inferred as List<Int>
val empty: List<Int> = []            // desugars to List::new()
```

- `[1, 2, 3]` is sugar for `List::from_array(...)`.
- The type of the list is inferred from context or from the element types.
- Empty list `[]` requires a type annotation (can't infer `T` from nothing).

### 6.2 Map and Set literals (v1+)

```axiom
val m: Map<String, Int> = ["a": 1, "b": 2]   // desugars to Map::from_pairs(...)
val s: Set<Int> = {1, 2, 3}                   // desugars to Set::from_array(...)
```

- Map literals use `[key: value, ...]` syntax.
- Set literals use `{elem, ...}` syntax (may conflict with block syntax — needs grammar
  disambiguation; see §16.1 grammar sketch).

---

## 7. Migration path (built-in → library)

### 7.1 v0: Built-in `List<T>` (temporary)

Before generics and traits exist, `List<T>` is a compiler built-in:
- `Ty::Builtin("List", vec![element_ty])` — name-based, extensible.
- Flat/inline storage, container-level refcount.
- Hard-coded subscript for indexing.
- Literal syntax `[1, 2, 3]` hard-coded in the parser.

### 7.2 v1: Generics + traits exist

Once generics (§3.6) and traits (§3.5) are implemented:
- Implement `Deinit`, `Hashable`, `Equatable` traits.
- Implement `DynamicBuffer` primitive.
- Implement subscript declarations with `yield`.

### 7.3 v2: Migrate `List<T>` to library type

- Rewrite `List<T>` as a library struct backed by `DynamicBuffer`.
- Remove `Ty::Builtin("List", ...)` from the compiler.
- `List<T>` becomes `Ty::Nominal(def_id_for_List, [element_ty])`.
- Map, Set, OrderedMap, OrderedSet are library types from day one (never built-in).

### 7.4 The migration is mechanical

The v0 built-in is designed to be **structurally identical** to the v2 library type:
- Same layout (ptr + len + cap, flat elements, refcount header).
- Same subscript behavior (yield-based projections, disjoint storage rule).
- Same element lifecycle (forward-order destruction, push/pop semantics).
- Same literal syntax (`[1, 2, 3]`).

The migration is: move the built-in implementation to a `.ax` file, replace hard-coded
type checker entries with generic resolution, done. No semantic changes.

---

## 8. Testing spec

### 8.1 The six layers (mirroring lexer, parser, HIR, type checker)

| Layer | What it is | The hole it closes |
|---|---|---|
| **1. Canonical dump** | One serializer for collection IR ops, exposed as a CLI command and used by the test oracle | "I can't see what collection operations the compiler generated" |
| **2. Golden snapshots** | `.ax` fixtures + checked-in `.collection` goldens, globbed by one test | "a change silently broke collection codegen" |
| **3. Coverage invariants** | Drift guard (every collection operation has a codegen path) + element lifecycle completeness (every `Deinit::drop` call is accounted for) | **"a case I never imagined slipped through"** ← the core fear |
| **4. Diagnostics** | Ill-typed collection usage → specific error + span, snapshotted | "a type mismatch in collection usage is silently accepted" |
| **5. Fuzz / property** | Random operations on collections; assert no panic, no memory leaks, no use-after-free | "the unimagined case" |
| **6. Unit tests** | Pinpoint checks on fiddly atoms (bounds checking, refcount elision, FBIP reuse) | "the subtle collection bug broad tests gloss over" |

Layers **3 and 5 are the load-bearing pair**.

### 8.2 The canonical collection IR dump format (the contract)

One serializer produces this; the CLI prints it and the golden harness compares it.

#### Rules

- **One operation per line.** Two-space indentation per depth level.
- **Every collection op shows its type.** `ListPush(List<Int>, Int)`, `ListIndex(List<Int>, Int) -> Int`.
- **Refcount ops are visible.** `incref(buffer, 2)`, `decref(buffer, 1)`, `elided(refcount)`.
- **Element lifecycle ops are visible.** `init_element(T[3], value)`, `drop_element(T[3])`.
- **Deterministic:** same source ⇒ byte-identical output.
- **LF only**, pinned via `.gitattributes`.

#### Line grammar

```
<depth_indent><Op>(<args>) : <result_type> [refcount:<rc_info>]
```

#### Example

```
ListNew() : List<Int>
ListPush(List<Int>, 42) : Unit
  init_element(T[0], 42)
ListPush(List<Int>, 17) : Unit
  init_element(T[1], 17)
ListIndex(List<Int>, 0) : Int [refcount:elided]
  yield T[0]
ListDrop(List<Int>) : Unit
  drop_element(T[0])
  drop_element(T[1])
  deallocate(buffer)
```

### 8.3 Coverage invariants

#### 8.3.1 `collection_ops_complete(ir) -> Result<(), CoverageError>`

Asserts that every collection operation in the IR has a corresponding codegen path:
- `ListNew` → allocate empty buffer
- `ListPush` → init element, increment len
- `ListPop` → decrement len, move element out
- `ListIndex(let)` → bounds check, yield read
- `ListIndex(inout)` → bounds check, yield inout
- `ListDrop` → drop all elements, deallocate
- `incref` / `decref` → Perceus refcount operations

If any IR op has no codegen path, this invariant fails.

#### 8.3.2 `element_lifecycle_complete(ir) -> Result<(), LifecycleError>`

Asserts that every initialized element is eventually deinitialized:
- Every `init_element(T[i])` has a matching `drop_element(T[i])` on all paths.
- No element is dropped twice.
- No element is dropped before initialization.

This is a static check on the IR, not a runtime check.

#### 8.3.3 `refcount_balance(ir) -> Result<(), RcError>`

Asserts that every `incref` has a matching `decref` on all paths:
- The refcount of every heap allocation starts at 1.
- Every `incref` increments by exactly the stated amount.
- Every `decref` decrements by exactly the stated amount.
- On every path to the end of the allocation's lifetime, the refcount reaches 0.

#### 8.3.4 `no_use_after_drop(ir) -> Result<(), LifetimeError>`

Asserts that no element or buffer is accessed after it has been dropped or deallocated.

### 8.4 Golden snapshot fixtures

#### Collection lifecycle fixtures

| Fixture | Tests |
|---|---|
| `list_empty.ax` | Empty list creation and drop — no allocation, no-op drop |
| `list_push_pop.ax` | Push elements, pop them back — verify element lifecycle |
| `list_literal.ax` | `[1, 2, 3]` desugaring to `List::from_array` |
| `list_index_read.ax` | Read projection via subscript |
| `list_index_write.ax` | Mutable projection via subscript (`xs[1] += 10`) |
| `list_drop_with_refcounted_elements.ax` | `List<String>` — verify each string is decremented on drop |
| `list_nested.ax` | `List<List<Int>>` — verify outer drop triggers inner drops |
| `list_move_semantics.ax` | Passing a list to a `sink` parameter invalidates the original |
| `list_let_borrow.ax` | Passing a list to a `let` parameter — no refcount change |
| `list_inout_borrow.ax` | Passing a list to an `inout` parameter — mutation in place |

#### Perceus / FBIP fixtures

| Fixture | Tests |
|---|---|
| `list_refcount_elision.ax` | Unique list — refcount ops elided entirely |
| `list_refcount_shared.ax` | Shared list — refcount ops present |
| `list_map_reuse.ax` | `xs.map(...)` when xs is unique — buffer reused, no allocation |
| `list_map_copy.ax` | `xs.map(...)` when xs is shared — new buffer allocated |
| `list_append_reuse.ax` | `xs.append(ys)` when xs is unique — in-place append |
| `list_copy_on_write.ax` | Mutation of shared list — copy buffer first |

#### Exclusivity fixtures (extending Spike 0)

| Fixture | Tests |
|---|---|
| `list_disjoint_index.ax` | `swap(inout xs[0], inout xs[1])` — accepted, distinct elements |
| `list_variable_index.ax` | `swap(inout xs[i], inout xs[j])` — rejected, can't prove i≠j |
| `list_push_while_iterating.ax` | `loop x in xs { xs.push(x) }` — rejected, iterate-while-mutate |
| `list_index_self.ax` | `xs[i] = f(xs[i])` — accepted, eager eval releases read first |
| `list_nested_projection.ax` | `grid[i][j] = 0` — accepted, nested projections |

#### Map/Set fixtures (v1+)

| Fixture | Tests |
|---|---|
| `map_insert_get.ax` | Basic map operations |
| `map_overwrite.ax` | Insert same key twice — value updated |
| `map_literal.ax` | `["a": 1, "b": 2]` desugaring |
| `set_insert_contains.ax` | Basic set operations |
| `set_literal.ax` | `{1, 2, 3}` desugaring |

### 8.5 Fuzz / property tests

#### 8.5.1 `fuzz_collection_ops(seed) -> FuzzResult`

Generate random sequences of collection operations (push, pop, index, drop, clone) on
random types. Assert:
- No panic at any point.
- No memory leak (all allocations freed).
- No use-after-free (all accesses are to live allocations).
- No double-free (each allocation freed exactly once).
- Element lifecycle is correct (every pushed element is eventually dropped).

#### 8.5.2 `fuzz_refcount_operations(seed) -> FuzzResult`

Generate random sequences of share/clone/drop operations on collections. Assert:
- Refcount is always non-negative.
- Refcount reaches 0 exactly when the last reference is dropped.
- Elision is sound (eliding refcount ops produces the same observable behavior as not
  eliding them).

#### 8.5.3 `fuzz_exclusivity_with_collections(seed) -> FuzzResult`

Generate random sequences of borrowing operations (let, inout, subscript, push, pop) on
collections. Assert:
- The exclusivity checker never panics.
- Accepted programs never exhibit undefined behavior.
- Rejected programs are correctly rejected (no false negatives).

### 8.6 Unit tests

#### Element lifecycle

| Test | What it verifies |
|---|---|
| `test_push_initializes_element` | `push(x)` initializes the slot, not memcpy |
| `test_pop_moves_element_out` | `pop()` returns ownership to caller, buffer loses it |
| `test_drop_walks_elements_forward` | Drop destroys elements in order 0, 1, 2, ... |
| `test_drop_empty_list_noop` | Dropping an empty list is a no-op (no allocation) |
| `test_drop_with_refcounted_elements` | Dropping `List<String>` decrefs each string |

#### Refcount behavior

| Test | What it verifies |
|---|---|
| `test_refcount_starts_at_one` | New list has refcount 1 |
| `test_refcount_increments_on_share` | Passing to `let` parameter increments (if not elided) |
| `test_refcount_decrements_on_drop` | Dropping a reference decrements |
| `test_refcount_elision_on_unique` | Unique list has no refcount ops in IR |
| `test_refcount_elision_on_move` | Moved list has no refcount ops (ownership transferred) |

#### Subscript projections

| Test | What it verifies |
|---|---|
| `test_subscript_read_returns_element` | `xs[0]` returns the element at index 0 |
| `test_subscript_write_mutates_in_place` | `xs[0] = 42` changes the element |
| `test_subscript_bounds_check_panics` | `xs[999]` on a 3-element list panics |
| `test_subscript_disjoint_indices_accepted` | `swap(inout xs[0], inout xs[1])` compiles |
| `test_subscript_variable_indices_rejected` | `swap(inout xs[i], inout xs[j])` fails |

#### FBIP / reuse

| Test | What it verifies |
|---|---|
| `test_map_reuses_buffer_when_unique` | `xs.map(f)` when xs is unique → no allocation in IR |
| `test_map_copies_buffer_when_shared` | `xs.map(f)` when xs is shared → allocation in IR |
| `test_append_reuses_buffer_when_unique` | `xs.append(ys)` when xs is unique → in-place |
| `test_functional_update_in_place` | Functional-looking code compiles to mutation |

#### Type system

| Test | What it verifies |
|---|---|
| `test_list_type_is_nominal` | `List<Int>` is `Ty::Nominal(def_id, [Ty::Int])` |
| `test_list_requires_deinit_bound` | `List<T>` enforces `T: Deinit` |
| `test_map_requires_hashable_equatable` | `Map<K, V>` enforces `K: Hashable + Equatable` |
| `test_list_literal_type_inference` | `[1, 2, 3]` infers `List<Int>` |
| `test_empty_list_requires_annotation` | `[]` alone is a type error |

---

## 9. Spec updates required

When this design is implemented, update `DESIGN_SPEC.md`:

1. **§3.2:** Replace the one-line list with a reference to this doc. Tag `List<T>` as
   `[Decided — library type on DynamicBuffer, Deferred → v1 implementation]`. Tag
   `Map<K,V>`, `Set<T>`, `OrderedMap<K,V>`, `OrderedSet<T>` as
   `[Decided — library type, Deferred → v1, requires Hashable/Equatable/Ord traits]`.

2. **§4.5:** Add subsection "Container refcounting" — how `incref`/`decref` propagate
   to elements when `T` is refcounted.

3. **§4.8:** Clarify that `List<T>` struct is stack-allocated; the buffer is heap-allocated.

4. **§13.4:** Add `DynamicBuffer` to the IR value representation.

5. **§14 roadmap:** Add `DynamicBuffer` primitive to v1 scope; `List<T>` library migration
   to v2 scope; `Map<K,V>`/`Set<T>` library types to v1 scope.

---

## 10. Dependency graph

```
Generics (§3.6) ──────────┐
                           ├──→ DynamicBuffer primitive ──→ List<T> library type
Traits (§3.5) ────────────┤                                  │
                           ├──→ Deinit trait ─────────────────┘
                           │
                           ├──→ Hashable + Equatable traits ──→ Map<K,V> library type
                           │                                   Set<T> library type
                           │
Subscripts (§4.4) ─────────┴──→ subscript declarations ──────→ List subscript projections

Perceus (§4.5) ────────────────→ refcount insertion ─────────→ container refcounting

All of the above ──────────────→ Path A fully implemented
```

**Critical path:** Generics → Traits → Deinit → DynamicBuffer → List<T>.
Map/Set branch off at Traits → Hashable/Equatable → Map/Set (parallel with List).

---

## 11. Honest open questions

| # | Question | Status |
|---|----------|--------|
| 1 | **Set literal syntax `{1, 2, 3}` vs block syntax `{ ... }`** — grammar ambiguity? | **Open** — needs grammar disambiguation rule (§16.1) |
| 2 | **Map literal syntax `["a": 1]` vs list-of-tuples `[("a", 1)]`** — disambiguation? | **Open** — `:` inside `[` is the signal, but needs grammar rule |
| 3 | **Element destruction order** — forward (0→n) or reverse (n→0)? | **Decided: forward** — matches Rust's Vec, natural for contiguous buffer |
| 4 | **Should `DynamicBuffer` be user-accessible?** | **Deferred → v2** — initially compiler-internal, expose later for power users |
| 5 | **Small-buffer optimization** (inline storage for small lists)? | **Deferred → post-v1** — premature until profiling shows it matters |
