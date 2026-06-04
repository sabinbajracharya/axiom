# Collection Type Design Research for Axiom

> Research date: 2026-06-04
> Context: Axiom uses Mutable Value Semantics (MVS) + Perceus compile-time reference counting. No GC, no lifetime annotations, deterministic memory safety.

## 1. How Other Languages Do It

### 1.1 Hylo (direct MVS ancestor)

**Declaration:** `Array<Element>` is a **library type**, not compiler built-in. Defined in the standard library.

```hylo
public type Array<Element: SemiRegular>: SemiRegular {
  var storage: DynamicBuffer<Int, Element>
  // ... methods ...
}
```

**Layout:** `DynamicBuffer<Header, Element>` is a raw `MemoryAddress` pointer to a heap allocation containing:
- A header (`{ capacity: Int, count: Int }`)
- Followed by `capacity` contiguous `Element` values (inline, not boxed)

**Element ownership:** Elements are stored **flat/inline**. The container is responsible for initializing/deinitializing elements (`DynamicBuffer`'s deinitializer explicitly does NOT deinitialize elements — the container must do it first). This is manual memory management behind a safe API.

**Subscripts:** Use `yield` for projections (read and inout). The subscript suspends, lends the element, and resumes.

**Key insight for Axiom:** Hylo proves you can build collections as library types in an MVS language. The compiler doesn't need to know about `Array` — it just needs to support `DynamicBuffer` (raw memory) and subscripts with `yield`.

### 1.2 Koka (Perceus source)

**Declaration:** `vector<a>` is an **opaque extern type** — compiler built-in, not definable in the language. `list<a>` is a regular algebraic type (Nil/Cons).

```koka
pub type vector<a>   // opaque, compiler-known
pub type list<a>
  con Nil
  con Cons(head:a, tail:list<a>)
```

**Layout:** Vectors are backed by C runtime functions (`kk_vector_alloc`, `kk_vector_at_borrow`). Elements are **boxed** — each element is a heap-allocated, refcounted value. The vector itself is a flat array of pointers to boxed values.

**Element ownership:** Perceus handles refcounting at the **element level**. When a vector is dropped, each element's refcount is decremented. When an element is extracted, its refcount is incremented.

**Reuse analysis:** Perceus's killer feature — when the compiler proves a vector has refcount 1 (unique ownership), operations like `map` or `append` compile to in-place mutation. No CoW runtime check needed.

**Key insight for Axiom:** Koka's approach is "opaque built-in + boxed elements + Perceus reuse." This is the simplest path for the compiler but sacrifices cache locality (every element is a pointer chase).

### 1.3 Swift

**Declaration:** `Array<T>` is a **generic struct** with compiler magic. It's a library type that gets special treatment (literal syntax, bridging, etc.).

```swift
public struct Array<Element> {
    internal var _buffer: _ArrayBuffer<Element>
}
```

**Layout:** `_ArrayBuffer` is a refcounted heap object (`ManagedBuffer`). Elements stored **inline** for value types, as refcounted pointers for reference types. The buffer has a single refcount (container-level).

**Element ownership:** For value types (structs, enums), elements are copied into the buffer — no per-element refcount. For reference types (classes), each element is a refcounted pointer. The buffer itself is CoW-managed.

**CoW mechanism:** On mutation, `isKnownUniquelyReferenced()` checks if the buffer has a single owner. If not, the buffer is copied before mutation. This is a **runtime check** on every mutation.

**Key insight for Axiom:** Swift's CoW is the "safe but with runtime overhead" approach. Every mutation pays for an atomic refcount check. Perceus avoids this by doing the analysis at compile time.

### 1.4 Rust

**Declaration:** `Vec<T>` is a **pure library type** — no compiler magic at all. Defined entirely in `std`.

```rust
pub struct Vec<T> {
    ptr: *mut T,
    len: usize,
    cap: usize,
}
```

**Layout:** 3 words (24 bytes on 64-bit). Elements stored **inline/flat** in a contiguous heap allocation. No header — length and capacity are in the `Vec` struct itself.

**Element ownership:** Tracked by the borrow checker. `Vec<T>` owns its elements. When dropped, it drops all `len` elements then deallocates. No runtime refcounting.

**Key insight for Axiom:** Rust proves you can have zero-overhead flat storage with library types. But Rust has the borrow checker — Axiom replaces that with Perceus, which means we need refcounting where Rust has none.

### 1.5 Lobster

**Declaration:** Vectors are **runtime built-in** (`RTT_VECTOR` in the VM). Not a library type.

**Layout:** Heap-allocated buffer with refcount header. Elements stored inline. Container-level refcounting (not per-element).

**Value semantics:** Lobster uses value semantics by default. Assignment copies values; containers are refcounted. When a container's refcount hits 1, mutations can happen in-place (similar to Perceus reuse, but runtime).

**Key insight for Axiom:** Lobster is the closest to Axiom's model (value semantics + refcounting). Its approach of "built-in container + container-level refcount + runtime uniqueness check" is the simplest viable path.

### 1.6 Zig

**Declaration:** No built-in dynamic collection. `[N]T` (arrays) are compile-time sized. `[]T` (slices) are fat pointers that don't own memory. `std.ArrayList` is the dynamic collection (library type).

**Layout:** `ArrayList` stores `{ ptr, len, cap }` — identical to Rust's `Vec`. Elements inline/flat.

**Key insight for Axiom:** Zig's separation of "array" (fixed-size, inline) vs "slice" (fat pointer, borrowed) vs "ArrayList" (owned, dynamic) is clean. Axiom could follow this pattern: `Array<N, T>` for fixed-size, `List<T>` for dynamic.

---

## 2. The Design Dimensions

### 2.1 Compiler Built-in vs Library Type

| Approach | Pros | Cons |
|----------|------|------|
| **Compiler built-in** (Koka, Lobster) | Simplest compiler implementation; can optimize hard-coded patterns | Inflexible; can't extend or replace; "one obvious way" but also "only way" |
| **Library type on raw memory primitive** (Hylo) | Flexible; users can build similar types; compiler stays small | More compiler surface area (need the raw memory primitive + subscripts) |
| **Library type with compiler magic** (Swift) | Best of both worlds in theory | Worst of both in practice — magic is invisible, hard to reason about |

### 2.2 Element Storage: Flat vs Boxed

| Approach | Pros | Cons |
|----------|------|------|
| **Flat/inline** (Rust, Hylo, Swift-value-types) | Cache-friendly; no pointer chasing; O(1) random access | Complex element lifecycle (must init/deinit in-place); container's `deinit` must walk elements |
| **Boxed** (Koka) | Simple refcounting (each element independent); simple container `deinit` | Pointer indirection on every access; poor cache locality; allocation per element |
| **Hybrid** (Swift mixed) | Optimal for each type | Complex implementation; two code paths |

### 2.3 Mutation Strategy: CoW vs Perceus Reuse vs Borrow-checker

| Approach | Pros | Cons |
|----------|------|------|
| **CoW** (Swift) | Simple mental model; well-understood | Runtime check on every mutation (atomic refcount read); false sharing can cause unnecessary copies |
| **Perceus reuse** (Koka, Axiom's spec) | Zero-cost when unique; compile-time analysis; no runtime checks | Complex compiler; reuse analysis must be correct; "best-case" percentages in §15 |
| **Borrow checker** (Rust) | Zero runtime cost; deterministic | Not Axiom's model — we explicitly chose no lifetimes |

### 2.4 Type System Representation

| Approach | Pros | Cons |
|----------|------|------|
| **`Ty::List(Box<Ty>)` etc.** | First-class in type checker; pattern matching on types is easy | Bakes specific collections into the compiler forever |
| **`Ty::Builtin(name, args)`** | Extensible; can add new built-ins without changing `Ty` | Less type-safety in the compiler; name-based lookup |
| **`Ty::Nominal(type_id, args)` with built-in IDs** | Uniform; works for user types too; built-ins just have special IDs | Most complex; needs a type registry |

---

## 3. Paths for Axiom

### Path A: Hylo-style (Library type on raw memory primitive)

**What it means:**
- `List<T>` is a library type wrapping a `DynamicBuffer`-like primitive
- The compiler provides: raw heap allocation, `yield`-based subscripts, MVS
- The standard library defines `List<T>`, `Map<K,V>`, `Set<T>` using those primitives
- Type system: `Ty::Nominal` with a special built-in ID

**Why:**
- Matches Hylo (the direct ancestor of Axiom's MVS model)
- Keeps the compiler small — collections are library code
- Users can build their own collection types with the same primitives
- The §4.4 subscript design already assumes this pattern (`impl<T> List<T> { subscript... }`)

**Why not:**
- More complex standard library implementation
- Need to define the `DynamicBuffer` equivalent (raw memory + init/deinit protocol)
- Map/Set need hash/compare — those need trait support (v1+)

**My take:** This is the right long-term answer. It's what the spec already implies (§4.4 shows `List<T>` as a user-definable type with subscripts). But it requires generics + traits to be working, which is v1 territory.

### Path B: Koka-style (Opaque built-in + boxed elements)

**What it means:**
- `List<T>` is an opaque type known to the compiler
- Elements are boxed (each element is a refcounted pointer)
- The compiler generates `incref`/`decref` for elements
- Type system: `Ty::List(Box<Ty>)` or `Ty::Builtin("List", vec![ty])`

**Why:**
- Simplest to implement — the compiler already has Perceus, just apply it to elements
- No need for a `DynamicBuffer` primitive
- Works immediately, even without full generics/traits
- Koka proves this works with Perceus

**Why not:**
- Boxed elements = pointer indirection on every access = poor cache locality
- `List<Int>` would store `Int` values as heap-allocated boxes — absurd overhead for small value types
- Can't be changed later without breaking the memory model
- Violates the "zero-cost abstractions" promise of Path A

**My take:** This is the fast path that creates a permanent performance problem. A `List<Int>` that boxes every integer is not systems-language material. Koka gets away with it because it's a research language targeting functional programming idioms. Axiom targets the Rust/Swift perf tier.

### Path C: Hybrid built-in (Flat storage + Perceus on the container)

**What it means:**
- `List<T>` is compiler-built-in with flat/inline element storage
- The container has a single refcount (container-level, not per-element)
- When `T` itself is refcounted (e.g., `List<String>`), the elements' refcounts are managed separately
- CoW-style: when the container is shared and a mutation is attempted, copy the buffer
- But unlike Swift's CoW, Perceus can often prove uniqueness at compile time and elide the check
- Type system: `Ty::List(Box<Ty>)` with known layout

**Why:**
- Cache-friendly (flat storage)
- Simple refcounting (one refcount per container, not per element)
- Perceus reuse analysis can eliminate most CoW checks at compile time
- Works before generics/traits are fully implemented
- Closest to what the spec's §4.4 already describes (the `self.buffer` field)

**Why not:**
- Bakes `List<T>` into the compiler — can't replace it
- Element lifecycle is complex: when `List<String>` is dropped, must `decref` each `String`
- When `T` is a struct with refcounted fields, the destructor must walk the element
- The compiler needs to know about element destructors — this requires some form of `Drop`/`Deinit` trait

**My take:** This is the pragmatic v0/v1 answer. It gets collections working with good performance, and the compiler complexity is bounded (one refcount per container, flat storage, Perceus elision). The "baked into compiler" downside is real but manageable — Axiom's "one obvious way" philosophy means we *want* a canonical `List<T>`.

### Path D: Staged — Builtin now, library later

**What it means:**
- v0/v1: `List<T>` is compiler-built-in (Path C)
- v1/v2: Introduce a `Collection` trait + `DynamicBuffer` primitive
- The built-in `List<T>` becomes a library type backed by `DynamicBuffer`
- Other collections (`Map`, `Set`) are library types from the start
- Type system: start with `Ty::List`, migrate to `Ty::Nominal`

**Why:**
- Gets something working fast (v0 needs a list to be useful)
- Doesn't lock into the built-in approach forever
- Can validate the `DynamicBuffer` primitive against the real `List<T>` implementation
- Map/Set don't need to be built-in — they can be library types from day one (just need hash/compare)

**Why not:**
- Migration cost — changing how `List<T>` works internally could break code
- Two phases of compiler complexity
- Risk of the "temporary" built-in becoming permanent (it always does)

**My take:** This is the most realistic path. It acknowledges that v0 *needs* a list type but the final design should be library-based. The key discipline is: design the built-in `List<T>` *as if* it were a library type (flat storage, subscript projections, explicit init/deinit) so the migration is mechanical, not a rewrite.

---

## 4. Recommendation

**Path D (Staged: Builtin now, library later)** with these specifics:

### For v0 (immediate):
- `List<T>` is compiler-built-in with flat/inline storage
- Container-level refcount (Perceus on the buffer, not per-element)
- `yield`-based subscripts as described in §4.4
- `incref`/`decref` on the buffer; when `T` is refcounted, the destructor walks elements
- Type system: `Ty::Builtin("List", vec![element_ty])` — extensible name-based approach
- Literal syntax: `[1, 2, 3]` desugars to `List::from_array(...)` or similar

### For v1 (with generics + traits):
- Introduce a `Deinit` trait (destructor) — the compiler calls `deinit` on elements when the container is dropped
- Introduce a `Hashable`/`Equatable` trait — enables `Map<K,V>` and `Set<T>` as library types
- `Map<K,V>` and `Set<T>` are library types from the start (no compiler magic)
- `OrderedMap`/`OrderedSet` are also library types

### For v2 (migration):
- Introduce `DynamicBuffer<Header, Element>` as a library-accessible primitive
- Migrate `List<T>` from built-in to library type
- Type system migrates from `Ty::Builtin` to `Ty::Nominal`

### Element lifecycle rules:
1. **`List<Int>`** — elements are flat, no per-element refcount. Buffer `decref` frees the buffer and all elements are trivially dropped.
2. **`List<String>`** — elements are flat but `String` is refcounted. Buffer drop walks elements and calls `decref` on each `String`.
3. **`List<List<Int>>`** — outer list's drop walks inner lists and calls `decref` on each. Inner lists' drops are triggered when their refcount hits 0.

This means the compiler needs a `drop_element(T)` codegen path for container types — which is the `Deinit` trait in all but name.

### Memory layout (concrete):
```
List<T> (stack/inline):
┌──────────────┐
│ buffer: *T   │ ──────► Heap allocation:
│ len: usize   │         ┌─────────────────┐
│ cap: usize   │         │ refcount: usize  │
└──────────────┘         │ len: usize       │
                         │ cap: usize       │
                         │ T[0]             │
                         │ T[1]             │
                         │ ...              │
                         │ T[cap-1]         │
                         └─────────────────┘
```

This is Rust's `Vec<T>` layout with a refcount header prepended. When Perceus proves uniqueness, the refcount is elided and it compiles to exactly Rust's `Vec<T>`.

---

## 5. Spec Updates Needed

1. **§3.2:** Expand to describe `List<T>` as compiler-built-in with flat storage, container-level refcount, and subscript projections. Tag `Map<K,V>`, `Set<T>` as `[Deferred — v1, library type]`.

2. **§4.5:** Add a subsection on "Container refcounting" — how `incref`/`decref` propagate to elements when `T` is itself refcounted.

3. **§4.8:** Clarify that `List<T>` is always heap-allocated (the buffer), but the `List` struct itself (ptr + len + cap) can be stack-allocated.

4. **§13.4:** Add `Ty::Builtin(name, args)` to the IR type representation.

5. **§14 roadmap:** Add `List<T>` built-in to v0 scope; `Map<K,V>`/`Set<T>` library types to v1 scope; `DynamicBuffer` primitive + `List` migration to v2 scope.
