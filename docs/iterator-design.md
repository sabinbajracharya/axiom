# Iterator Design — Loop, Adapt, Compose

> **Status:** Draft for review. Not yet authoritative — decisions below are proposals
> for discussion before code is written.
>
> **Decisions baked in:** `Iterator<T>` uses a generic type parameter (no associated types —
> deferred to v2), `next()` takes `inout self`, `loop x in xs` desugars to `into_iter()` +
> `next()` calls, `List<T>` is iterable through a `ListIter<T>` struct, adapters are
> default methods on the trait.
>
> **Prerequisites:** traits with default methods (`traits-design.md` — `fn next(inout self)` is
> the required method; adapters like `count`, `fold` are default methods), generic trait
> declarations (the `Iterator<T>` syntax), the HIR desugar pass
> (`hir-desugar-pass-design.md` — `loop x in` is the second sugar to go through the pass),
> closures (`DESIGN_SPEC.md` §8.2 — needed for `map`/`filter`/`fold` adapters; without them,
> only `next()` + `count()` work).
>
> **Companion docs:** [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §7.1 (loop forms), §11 (stdlib —
> `Iterator` trait + adapters), [`traits-design.md`](traits-design.md) (trait machinery),
> [`hir-desugar-pass-design.md`](hir-desugar-pass-design.md) (the desugar pipeline),
> [`lang-items-and-desugaring-design.md`](lang-items-and-desugaring-design.md) (lang-item
> registry), [`collection-type-design.md`](collection-type-design.md) (List/Map as consumers),
> [`modules-design.md`](modules-design.md) (`core/iter.ax` in module layout),
> [`builtin-to-stdlib-migration.md`](builtin-to-stdlib-migration.md) (stdlib layout).

---

## 0. The concern this answers

The language has a `loop x in iterable { }` syntax (§7.1) that is **parsed but dead**—the
parser builds `LoopExpr` nodes with `is_iterator()`, `iter_pattern()`, `iter_iterable()`, but
the downstream pipeline (desugar, typeck, IR) has no `Iterator` trait to wire them to. Every
iteration today is index-based (`loop i in 0..n`), which is fine for arrays but breaks the
abstraction for maps, custom collections, lazy sequences, or chained operations.

Meanwhile, `List<T>` and `Map<K,V>` are real library types with no way to iterate over their
elements except raw index loops. Every language in Axiom's reference set (Swift, Kotlin, Rust,
Go, Zig) has first-class iteration over collections. Without it, Axiom collections are
incomplete and every loop over a Map requires the user to manually manage a key list.

The fear: **we ship a "statically typed systems language with collections" where every
iteration is an index loop**, and the `loop x in` syntax becomes a permanent parse-only
artifact—a visible promise the language doesn't keep.

---

## 1. The design, stated plainly

### 1.1 The `Iterator<T>` trait

A single trait, defined in `core/iter.ax` (new file, always available):

```ax
/// A value that yields items of type `T` on demand.
/// Call `next()` until it returns `None`.
pub trait Iterator<T> {
    /// Advance the iterator and return the next element, or `None` if exhausted.
    /// `inout self` because iteration mutates internal position state.
    fn next(inout self) -> Option<T>;
}
```

Key design decisions:

- **`Iterator<T>` with a type parameter, not `Iterator { type Item }`** — associated types
  are deferred to v2 (`traits-design.md` §1.3). Generic `Iterator<T>` is the v1 form.
  This means a type can only implement `Iterator` for one `T` at a time, which is
  sufficient for v1 (no type can meaningfully yield two different item types).
  [Decided — follows the deferred-associated-types decision.]

- **`fn next(inout self) -> Option<T>`** — `inout self` borrows the iterator exclusively
  and mutably (like Rust's `&mut self` in MVS terms). The exclusivity rule (§4.3) ensures
  you can't call `.next()` while another borrow is active, which catches iterator
  invalidation at compile time — the Spike 0 confirmed this works (§4.10).
  [Decided — follows MVS conventions.]

- **Returns `Option<T>`** — `Option` is the core optional-value type. `Some(value)` means
  a yielded element; `None` means the iterator is exhausted. This ties into `match`
  exhaustiveness (§7.2) — the compiler enforces that both arms are handled.
  [Decided — follows the existing `Option` type.]

- **Part of `core` (implicit prelude)** — `Iterator` is in every scope without `use`.
  Same as `Option`, `Result`, `Deinit`, `Equatable`, etc. The trait name and its adapter
  methods resolve without imports.
  [Decided — listed in DESIGN_SPEC.md §11 as core: `traits (Deinit, …, Iterator + adapters)`.]

### 1.2 The `loop x in iterable { }` desugaring

The `loop x in xs { body }` form (§7.1) is desugared in the HIR desugar pass
(`docs/hir-desugar-pass-design.md`). Two regimes:

**Regime A — `xs` implements `Iterator<E>`:**
If the iterable expression's type implements `Iterator<E>`, the desugar produces:

```
// loop x in xs { body }
// ─────────────────────
loop {
    match xs.next() {
        Some(x) => { body }
        None    => break
    }
}
```

No temporary, no `into_iter` call. The variable `xs` is borrowed via `inout self` through
the exclusivity rule.

**Regime B — `xs` has an `into_iter` method returning an iterator:**
If `xs`'s type has an associated function `into_iter` that returns a type implementing
`Iterator<E>`, the desugar produces:

```
// loop x in xs { body }
// ─────────────────────
val __iter = xs.into_iter()
loop {
    match __iter.next() {
        Some(x) => { body }
        None    => break
    }
}
```

The resolution order is: try direct `Iterator<E>` impl first, then look for `into_iter`.
This is checked at desugar time via the lang-item `def_id` for `Iterator` (the trait
itself) and `into_iter` (the conversion method), both resolved during lang-item resolution.

[Decided — "`loop … in` → `next()` loop + `Iterator` lang item"
(`lang-items-and-desugaring-design.md` §5).]

### 1.3 Consuming (`sink`) vs reading (`let`) vs mutating (`inout`) iteration

Following the three-calling-convention design of MVS (§4.2), the loop form distinguishes
iteration intent by the binding convention on the loop variable:

```ax
loop x in xs      { }   // x: let  — read-only access to each element
loop inout x in xs { }   // x: inout — mutable access to each element
loop sink x in xs  { }   // x: sink — elements are moved out (consuming iteration)
```

**`loop x in xs` (read, default):**
Desugars as described in §1.2. The element `x` is yielded as a `let` binding — the caller
can read but not modify or store it. The iterator yields elements by value (copies or moves,
depending on the element type). This is the default when no convention keyword is written.

**`loop inout x in xs` (mutable):**
For indexable collections (types that implement `subscript` with `inout self`), this
desugars to index-based iteration:

```
// loop inout x in xs { body }
// ───────────────────────────
var __i = 0
loop if __i < xs.count() {
    inout x = xs[__i]
    body
    __i = __i + 1
}
```

This does NOT go through the `Iterator` trait — it uses the subscript convention directly.
It works for any type with `subscript(inout self, index: Int) -> inout T`.
[List, Map's entries, and user-defined collections all qualify.]

**Why `inout` iteration bypasses `Iterator`:** The `Iterator` trait yields elements by
value (`fn next(inout self) -> Option<T>` returns `T`, not `inout T`). Mutable iteration
needs `inout` projection of each element, which requires the collection's structural
knowledge—a trait can't abstract over "return a pointer to the i-th slot" without
associated types or GATs, both deferred to v2. Index-based access with `subscript` is
the honest v1 solution.

[Decided — `loop x in` and `loop sink x in` go through `Iterator`;
`loop inout x in` goes through `subscript` + index loop.]

**`loop sink x in xs` (consuming):**
Same desugar as the default `loop x in xs` but the binding convention for each element
is `sink` — the element is moved out of the iterator. For `ListIter<T>`, this means each
element is removed from the backing buffer as it's yielded (or the whole buffer is
consumed at once and elements are yielded from the owned storage).

For v1, the material difference between `loop x in xs` and `loop sink x in xs` is:
- `loop x in xs` — elements are yielded as `let` bindings; the element type `T` must
  implement `Deinit` (already a bound on `List<T>`), but the element is not consumed
  from the source.
- `loop sink x in xs` — elements are yielded as `sink` bindings; the caller takes
  ownership. The Iterator impl moves the element out of its storage.

### 1.4 Default adapter methods on `Iterator<T>`

The `Iterator<T>` trait carries default adapter methods that compose on top of `next()`.
Adapters that take a closure parameter (marked `|…|`) depend on the closure implementation
(§8.2) and are deferred until closures land. Adapters without closures work immediately.

**Phase 1 adapters (no closures, work immediately):**

```ax
pub trait Iterator<T> {
    fn next(inout self) -> Option<T>;

    /// Count how many elements remain. Consumes the iterator.
    fn count(inout self) -> Int {
        var n = 0
        loop if self.next() != None {
            n = n + 1
        }
        n
    }

    /// Return the first element (or `None` if empty). Consumes the iterator.
    fn first(inout self) -> Option<T> {
        self.next()
    }
}
```

**Phase 2 adapters (need closures, deferred):**

```ax
    /// Apply `f` to each element, yielding the results.
    fn map<S>(inout self, f: |T| -> S) -> MapIter<Self, T, S> { … }

    /// Keep only elements for which `f` returns `true`.
    fn filter(inout self, f: |T| -> Bool) -> FilterIter<Self, T> { … }

    /// Accumulate elements left-to-right, starting from `init`.
    fn fold<A>(inout self, init: A, f: |A, T| -> A) -> A { … }

    /// Reduce using `f`, or return `None` if empty.
    fn reduce(inout self, f: |T, T| -> T) -> Option<T> { … }

    /// Call `f` for each element (for side effects).
    fn for_each(inout self, f: |T|) { … }
```

**Design note on adapter return types:** `map` and `filter` return lazy wrapper structs
(`MapIter`, `FilterIter`) that each implement `Iterator<S>` / `Iterator<T>`, enabling
chaining: `xs.into_iter().map(f).filter(g).fold(0, |a, x| a + x)`. These wrapper structs
are defined in `core/iter.ax` alongside the trait. They are generic over the inner
iterator type and the closure — which means their concrete types are unnameable in user
code without `impl Trait` or `dyn Trait` (both deferred). For v1, this is acceptable:
users write the chain and type inference drives it; if they need to name the type, they
assign to a variable with an inferred type.

[Decided — adapters as default methods, lazy wrappers for `map`/`filter`, defer naming
to when `impl Trait` or `dyn Trait` lands.]

### 1.5 Migration: `List<T>` implements `Iterator<T>` via `ListIter<T>`

**The `ListIter<T>` struct** (defined in `std/collections/list.ax`):

```ax
/// An iterator over a `List<T>`. Created by `into_iter()`.
/// Yields elements in insertion order, consuming the list.
pub struct ListIter<T: Deinit> {
    list: List<T>,
    index: Int,
}

impl<T: Deinit> Iterator<T> for ListIter<T> {
    fn next(inout self) -> Option<T> {
        if self.index < self.list.count() {
            val result = self.list[self.index]
            self.index = self.index + 1
            Some(result)
        } else {
            None
        }
    }
}
```

**The `into_iter()` method** on `List<T>`:

```ax
impl<T: Deinit> List<T> {
    /// Consume the list and return an iterator over its elements.
    pub fn into_iter(let self) -> ListIter<T> {
        ListIter { list: self, index: 0 }
    }
}
```

**The `iter()` method** (read-only iteration, copies element metadata):

```ax
impl<T: Deinit> List<T> {
    /// Return an iterator over the elements without consuming the list.
    /// The backing buffer is refcounted by Perceus; this creates a shallow
    /// copy of the list metadata (count, cap, buffer pointer).
    pub fn iter(let self) -> ListIter<T> {
        ListIter { list: self, index: 0 }
    }
}
```

`iter()` works because in MVS + Perceus, passing `let self` to `iter()` creates a
shared refcount on the backing buffer. Both the original `List` and the `ListIter`'s
internal `List` share the buffer until one of them is dropped.

**No existing code breaks** — the new methods (`into_iter`, `iter`) are additive.
The existing index-based loop pattern (`loop i in 0..xs.count() { xs[i] }`) continues
to work and is not deprecated. The `Iterator` trait is opt-in: code that doesn't use
iteration doesn't change.

### 1.6 `Map<K,V>` iteration

`Map<K,V>` gets the same treatment via `MapIter<K,V>`:

```ax
pub struct MapIter<K: Hashable + Equatable + Deinit, V: Deinit> {
    map: Map<K, V>,
    index: Int,
}

impl<K, V> (MapIter<K, V>)
    where K: Hashable + Equatable + Deinit, V: Deinit
{
    pub fn next(inout self) -> Option<(K, V)> {
        // scan the backing arrays for the next occupied slot
        // starting from self.index, return key+value as a tuple
    }
}
```

Tuples `(K, V)` are the element type for Map iteration. Axiom does not have a separate
`Entry` type for v1 — `(key, value)` tuples are destructured in the loop:

```ax
loop (key, value) in map.into_iter() {
    print(key + ": " + value)
}
```

The tuple destructuring in the loop pattern works through the existing `match` pattern
machinery (§7.2). The Iterator yields `(K, V)` and the pattern `(key, value)` destructures it.

[Decided — Map iteration yields `(K, V)` tuples, no `Entry` type for v1.]

### 1.7 Ranges as iterators

The `0..n` range literal (`DESIGN_SPEC.md` §2.7) produces a `Range<Int>` value. When
used with `loop x in`, the range implements `Iterator<Int>`:

```ax
// Not a struct — Range is compiler-built or a lang-item type.
// For v1: `loop i in 0..n` where n is Int works through:
//
// Range<Int> implements Iterator<Int>
//   next() returns consecutive integers until >= end
```

Range iteration is the most common use case and must work without importing anything.
The `..` operator produces a `Range<T>` value; `Range<T>` implements `Iterator<T>` when
`T` is `Int` (and later `Float`). The range literal `0..n` does NOT consume a collection,
so `loop i in 0..n` is efficient — the `Range` struct holds start/end values and the
iterator advances by incrementing.

For v1, only `Int` ranges are required. `Float` ranges and `Char` ranges are deferred.

[Decided — v1: `Int` ranges implement `Iterator<Int>`, deferred: float/char ranges.]

---

## 2. What this does NOT include

| Feature | Status | Why |
|---------|--------|-----|
| Associated types (`type Item`) | `[Deferred → v2]` | Generic `Iterator<T>` works for v1; associated types add complexity without immediate benefit |
| `impl Trait` / `dyn Iterator` | `[Deferred → v1.1]` | Adapter chains use concrete (but unnameable) types; inference covers v1 |
| `IntoIterator` trait | `[Deferred → v2]` | `loop x in xs` desugars via lang-item lookup for `into_iter` on the concrete type, not a trait bound; a proper `IntoIterator` trait needs associated types |
| `LendingIterator` / streaming iterators | `[Deferred → v2]` | Needs GATs or lifetime parameters — way beyond v1 |
| `StepBy` / `Take` / `Skip` / `Peekable` adapters | `[Deferred → v2]` | The four core adapters (`map`, `filter`, `fold`, `reduce`) cover the common cases; more adapters are additive |
| `DoubleEndedIterator` (reverse iteration) | `[Deferred → v2]` | Needs the `next_back` protocol and `rev()` adapter |
| `ExactSizeIterator` | `[Deferred → v2]` | Optimisation for `count()` without consuming — not needed for correctness |
| `FusedIterator` | `[Deferred → v2]` | `next()` returning `None` after exhaustion is a convention, not a compiler-enforced guarantee |
| `collect()` adapter | `[Deferred → v2]` | Needs `FromIterator` trait (`List::from_iter`, `Map::from_iter`) — deferred with associated types |
| `sum()` / `product()` adapters | `[Deferred → v2]` | Needs `Add` / `Mul` traits |
| `max()` / `min()` adapters | `[Deferred → v2]` | Needs `Ord` bound on the adapter, which works in v1 — implement if needed |
| `all()` / `any()` adapters | `[Deferred → v2]` | Trivial with closures (`all(f) = !any(|x| !f(x))`) — deferred until closures land |
| `position()` / `enumerate()` adapters | `[Deferred → v2]` | `enumerate` needs tuple-yielding, which works — low priority |
| Mutating iteration via `inout self` on adapter chains | `[Deferred → v2]` | `map`/`filter` with `inout` closures needs deeper MVS integration |
| `chain()` adapter | `[Deferred → v2]` | Needs two inner iterator types — possible with concrete wrappers |
| `zip()` adapter | `[Deferred → v2]` | Needs two iterator types simultaneously — v2 |

---

## 3. Implementation phases

### Phase 1 — Core trait + `loop x in` desugar

**Goal:** `Iterator<T>` trait exists in `core/iter.ax`, `loop x in xs { }` desugars and
compiles for `List<T>`.

- [ ] **1.1** Create `core/iter.ax` with `Iterator<T>` trait + `next()` + `count()` + `first()`
- [ ] **1.2** Register `Iterator` as a lang item: add `LANG_ITERATOR` / `LANG_INTO_ITER` /
      `LANG_NEXT` to `resolver/src/lang.rs`, `REQUIRED_LANG_ITEMS`, `LangItems` struct,
      `set`/`get` dispatchers
- [ ] **1.3** Add `@lang` tags to the `Iterator` trait (`@lang("iterator")`), the `next`
      associated method (`@lang("iterator_next")`), and `into_iter` (`@lang("into_iter")`)
- [ ] **1.4** Wire `loop x in` desugaring in the HIR desugar pass (`axiom-hir/src/desugar.rs`):
      when `LoopKind::Iterator` is encountered, emit `val __iter = expr.into_iter()` +
      `loop { match __iter.next() { … } }`
- [ ] **1.5** Remove the `NotYetSupported` gate for iterator loops in typeck (or wherever
      it currently blocks)
- [ ] **1.6** Create `ListIter<T>` struct in `std/collections/list.ax` with `next(inout self)`
- [ ] **1.7** Add `into_iter(let self)` and `iter(let self)` methods on `List<T>`, returning
      `ListIter<T>`
- [ ] **1.8** Add tests: `loop x in list { }` e2e, `loop x in Range { }` e2e, golden
      snapshot of desugared HIR, unit tests for `ListIter::next`

**Tests pass:**
- `loop x in list { print(x) }` prints each element
- `loop x in list { }` iterating over an empty list is a no-op
- `list.into_iter().count()` returns `list.count()`
- `list.into_iter().first()` returns `list[0]`
- Compile error: `loop x in not_iterable { }` on a type without `Iterator` impl +
  no `into_iter`

### Phase 2 — Adapters without closures (count, fold)

**Goal:** Usable adapters on `Iterator<T>` that don't need closures.

- [ ] **2.1** Implement `fold` as a default method using a simple callback type that
      doesn't need closures — use a struct-based approach? **Actually: defer to Phase 3
      where closures land.** `fold` without closures needs a named function type, which
      doesn't exist yet.
- [ ] **2.2** (No-op — `count` and `first` are already in Phase 1)

### Phase 3 — Closure-dependent adapters

**Goal:** `map`, `filter`, `fold`, `reduce`, `for_each` work with closures.

- [ ] Prerequisite: closures (`DESIGN_SPEC.md` §8.2) are implemented end-to-end (parsed,
      type-checked, compiled, runnable)
- [ ] **3.1** Add `MapIter<Iter, T, S>` wrapper struct + `map` default method
- [ ] **3.2** Add `FilterIter<Iter, T>` wrapper struct + `filter` default method
- [ ] **3.3** Add `fold` default method (takes `|A, T| -> A`)
- [ ] **3.4** Add `reduce` default method (takes `|T, T| -> T`)
- [ ] **3.5** Add `for_each` default method (takes `|T|`)
- [ ] **3.6** Chaining tests: `xs.into_iter().map(f).filter(g).fold(0, |a, x| a + x)`
- [ ] **3.7** Golden snapshots for each adapter's desugared/compiled form

### Phase 4 — `inout` loop variant + Map iteration

**Goal:** `loop inout x in xs { }` desugars to index-based mutation; `Map<K,V>` is iterable.

- [ ] **4.1** Wire `loop inout x in xs { }` in the HIR desugar pass: emit `var __i = 0;
      loop if __i < xs.count() { inout x = xs[__i]; body; __i = __i + 1 }`
- [ ] **4.2** Add `MapIter<K, V>` struct in `std/collections/map.ax` with `next()`
- [ ] **4.3** Add `into_iter()` and `iter()` on `Map<K, V>`
- [ ] **4.4** Tuple destructuring in loop patterns: `loop (k, v) in map.into_iter() { }`
- [ ] **4.5** Tests: `loop inout x in xs { x = x + 1 }`, Map iteration, Map mutation
      during iteration is caught by exclusivity

### Phase 5 — Range iterator

**Goal:** `loop i in 0..n { }` uses the `Iterator` trait (not index-based).

- [ ] **5.1** Define `Range<T>` type (or verify it exists) with `Iterator<T>` impl for
      `Int` ranges
- [ ] **5.2** Wire `0..n` to produce a `Range<Int>` through normal type resolution
      (not ad-hoc)
- [ ] **5.3** Tests: `loop i in 0..10 { }`, `loop i in -5..5 { }`, nested range loops

---

## 4. Dependency graph

```
                      ┌────────────────┐
                      │ traits-design  │
                      │ (trait system) │
                      └───────┬────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
     ┌────────────────┐ ┌──────────┐ ┌──────────────┐
     │ generics-design│ │ closures │ │ lang-items   │
     │ (type params   │ │ (§8.2)   │ │ (resolver)   │
     │  on traits)    │ └────┬─────┘ └──────┬───────┘
     └────────────────┘      │              │
              │              │              │
              ▼              ▼              ▼
     ┌─────────────────────────────────────────┐
     │   Iterator trait in core/iter.ax        │
     │   (Phase 1 — no closures)               │
     └──────────────────┬──────────────────────┘
                        │
          ┌─────────────┴─────────────┐
          ▼                           ▼
┌──────────────────┐      ┌──────────────────────┐
│ ListIter<T>      │      │ HIR desugar pass     │
│ (Phase 1.6)      │      │ (loop x in → next()) │
└────────┬─────────┘      └──────────┬───────────┘
         │                           │
         ▼                           ▼
┌──────────────────┐      ┌──────────────────────┐
│ MapIter<K,V>     │      │ loop inout desugar   │
│ (Phase 4)        │      │ (Phase 4.1)          │
└──────────────────┘      └──────────────────────┘

         ─── Deferred until closures land ───
                        │
                        ▼
         ┌──────────────────────────┐
         │ map, filter, fold, reduce│
         │ (Phase 3 adapters)       │
         └──────────────────────────┘
```

---

## 5. Compiler architecture changes

| Crate | Change |
|-------|--------|
| `stdlib` (source) | New file: `stdlib/core/iter.ax` — `Iterator<T>` trait + adapter default methods + `MapIter`/`FilterIter`/`ListIter` structs (ListIter in `std/collections/list.ax`) |
| `resolver/src/lang.rs` | New lang items: `LANG_ITERATOR`, `LANG_ITERATOR_NEXT`, `LANG_INTO_ITER` — added to `REQUIRED_LANG_ITEMS`, `LangItems` struct, `set`/`get` dispatch |
| `resolver/src/lang.rs` | New constant: `ITERATOR` (type name string), `INTO_ITER` (method name) — drift-guarded same as `LIST` constants |
| `hir/src/desugar.rs` | New desugar rule for `LoopKind::Iterator`: emit `into_iter()` call + `next()` loop with `match Some/None`. Requires fresh `HirId` generation and `__iter_N` temp names (follow the list-literal pattern) |
| `hir/src/hir/mod.rs` | No structural changes needed — `LoopKind::Iterator` already exists |
| `typeck` | Remove `NotYetSupported` for Iterator loops; let desugared calls type-check normally through existing `next()` and `into_iter()` method resolution |
| `ir/lower` | No changes — iterator loops are desugared in HIR; IR sees plain calls |
| `vm` | No changes — the VM already handles method calls and `match`. Nothing new |
| All test crates | Golden snapshots for desugared Iterator loops; e2e tests for List/Map/Range iteration |

---

## 6. Testing strategy

Following the six-layer model from `collections-design.md` §8:

### 6.1 Layer 1 — Unit tests (`core/iter.ax` module tests, `ListIter` unit tests)

| Test | What it covers |
|------|---------------|
| `test_iterator_next_returns_elements` | `ListIter` yields elements in order |
| `test_iterator_next_returns_none_at_end` | After exhausting, `next()` returns `None` |
| `test_iterator_count` | `count()` equals number of elements |
| `test_iterator_first` | `first()` returns first element, `None` on empty |
| `test_iterator_empty_list` | Empty iterator yields `None` immediately |
| `test_into_iter_consumes_list` | After `into_iter()`, original list is consumed |
| `test_iter_shares_buffer` | `iter()` creates a shared buffer; both can live |

### 6.2 Layer 2 — Golden snapshots (desugared HIR)

| Golden file | Input | What it pins |
|---|---|---|
| `loop_iterator.hir` | `loop x in list { print(x) }` | Desugared form: `into_iter()` + `next()` loop + `match` |
| `loop_iterator_range.hir` | `loop i in 0..n { }` | Range desugar (or index-based when Range is a struct) |
| `list_into_iter_method.hir` | `list.into_iter()` | Single method call — no sugar, just verification |

### 6.3 Layer 3 — Coverage invariant

New invariant in `desugar_coverage.rs`:
- every `LoopKind::Iterator` node is eliminated before typeck (same as list literals)
- Adding a new `Expr` variant without handling it in the desugar pass fails the build

### 6.4 Layer 4 — End-to-end tests

| Test | What it verifies |
|---|---|
| `loop_iterate_list` | `loop x in [1, 2, 3]` prints each element |
| `loop_empty_list` | `loop x in []` is a no-op |
| `loop_iterator_count` | `list.into_iter().count() == list.count()` |
| `loop_iterator_first` | `list.into_iter().first() == list[0]` |
| `loop_map_iterate` | `loop (k, v) in map.into_iter() { }` yields entries |
| `loop_range` | `loop i in 0..5 { }` iterates 0 through 4 |
| `loop_inout` | `loop inout x in list { x = x + 1 }` modifies in place |
| `compile_error_non_iterable` | `loop x in 42 { }` fails — no `Iterator` impl |
| `compile_error_invalid_inout_target` | `loop inout x in not_indexable { }` fails |

### 6.5 Layer 5 — Diagnostics tests

| Test | What it verifies |
|---|---|
| `missing_iterator_lang_item` | Stdlib without `@lang("iterator")` reports `MissingLangItem` |
| `missing_into_iter` | Type without `into_iter` gives a clear error suggesting the fix |
| `iterator_invalidation` | Mutating `xs` during `loop x in xs` is caught by exclusivity (§4.3) |
| `loop_inout_non_indexable` | Error message mentions `subscript` requirement |

### 6.6 Layer 6 — Fuzz / property tests

| Property | Invariant |
|---|---|
| Re-typecheck | Desugaring iterator loops must not introduce type errors on valid code |
| Idempotence | Desugaring an already-desugared tree is a no-op |
| Count matches length | `list.into_iter().count() == list.count()` for all non-empty lists |
| First matches subscript | `list.into_iter().first() == list[0]` for all lists with at least one element |

---

## 7. Risks and mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Closures don't land in v1** | `map`, `filter`, `fold`, `reduce`, `for_each` are blocked | Phase 1 delivers `next()` + `count()` + `first()` + loop desugar — the core value. Adapters are purely additive; `loop x in xs` alone is a complete feature. If closures slip, adapters slip with them — no loss of the foundational work |
| **`into_iter` method resolution is fragile** | Desugar pass calls `.into_iter()` by name; a type with an unrelated `into_iter` method could collide | Mitigated by the lang-item mechanism: the compiler resolves `@lang("into_iter")` to a specific `DefId` and emits calls through the `DefId`, not by name string. No ambiguity. |
| **`loop inout x in xs` exclusivity** | The exclusivity checker could be too conservative and reject valid patterns | Spike 0 already validated this: `loop i in 0..n { xs[i]=… }` works; iterate-while-mutate is correctly caught. The index-based desugar for `inout` uses subscript projection which the spike confirmed handles correctly. |
| **Perceus refcounting on shared iter buffers** | `List::iter()` shares the heap buffer between List and ListIter; refcount operations at every `next()` could degrade performance | The refcounting is on the `[T]` buffer, not on the iterator. `ListIter` holds a copy of the buffer handle (a refcount bump). `next()` reads from the buffer at `index` — no refcount manipulation per element. Only the drop of `ListIter` or `List` decrements the refcount. |
| **Tuple destructuring in loop patterns** | `loop (k, v) in map.into_iter()` needs pattern matching on tuples | Axiom already has `match` with destructuring (§7.2). Tuple destructuring in loop patterns uses the same pattern machinery — the `ident ( ident , ident )` pattern syntax needs parser support, but the match/match-arm lowering already handles nested patterns. |
| **Range type doesn't exist yet** | `loop i in 0..n` has no `Range` struct to implement `Iterator` | Fallback: the existing `loop if` form (`loop if i < n { i = i + 1 }`) already works. The Range type can be a simple v0 struct added during Phase 5 — or we can keep range iteration as a compiler special case if the struct approach is too heavy for v1. |

---

## 8. Honest open questions

| # | Question | Status | Notes |
|---|----------|--------|-------|
| 1 | Does the `loop x in xs` desugar call `into_iter()` implicitly, or require the iterable to implement `Iterator` directly? | **Open** | Option A (direct impl): simpler desugar, no `into_iter` lang item. Option B (into_iter): follows Rust, enables `impl Iterator` on types that can't own their state. **Recommendation:** support both — check for direct `Iterator` impl first, fall back to `into_iter()`. |
| 2 | Should `loop x in xs` where `xs` is a `let` binding require explicit `xs.iter()`? | **Open** | Invoking `into_iter()` on a `let` binding would consume it, which may surprise users. The loop could implicitly call `.iter()` when the binding is `let` and `.into_iter()` when `sink`. But this adds complexity to the desugar. **Recommendation:** for v1, require explicit `into_iter()` or `iter()` for non-`inout` loops; `loop x in xs` only works on types that implement `Iterator` directly. Revisit when ux feedback arrives. |
| 3 | Can traits have default methods in the current compiler? | **Open** | `traits-design.md` says "default methods allowed" but this may not be implemented yet. If not, adapters become free functions in `core/iter.ax` (e.g. `fn count<T>(iter: T) -> Int where T: Iterator<T>`). Default methods are more elegant but not a blocker. |
| 4 | How does `.count()` work without consuming `self`? | **Open** | `count(inout self)` consumes the iterator position, which means after `count()`, the iterator is exhausted. This is the same behavior as Rust. If we want a non-consuming `count()`, defer to `ExactSizeIterator` in v2. |
| 5 | How do `let`/`inout`/`sink` conventions interact with the loop variable when using adapters? | **Open** | `xs.into_iter().map(f)` where the map closure captures by `let` — does the element arrive as `let` or `sink`? The closure's capture convention decides, not the loop variable convention. This needs real examples to settle. Defer to Phase 3. |
| 6 | Should `loop x in xs` be usable with `String` to iterate characters? | **Open** | String iteration (char-by-char) is a reasonable request but adds complexity (UTF-8 decoding, grapheme clusters). **Recommendation:** defer to v1.1 or until a `Chars` type exists. For v1, `loop x in "abc"` is a compile error (no `Iterator` impl for `String`). |
| 7 | Do we need a separate `Iterable<T>` trait (a `make_iterator()` factory), or is `Iterator<T>` directly on types sufficient? | **Open** | Rust has both `IntoIterator` and `Iterator`. Axiom's MVS model may not need the split — a type that can be iterated can just implement `Iterator<T>` directly (like `Range<Int>`). But for collections that need a *separate* iterator struct (like `List`), the conversion is a named method (`into_iter()` / `iter()`), not a trait. **Recommendation:** no `Iterable` trait for v1 — types implement `Iterator` directly, or provide `into_iter()` / `iter()` methods. Revisit if the pattern demands a trait bound. |

---

## 9. What success looks like

After all phases:

```ax
val xs: List<Int> = [10, 20, 30, 40, 50]

// — Read iteration —
loop x in xs.into_iter() {
    print(x)  // 10, 20, 30, 40, 50
}

// — Consuming iteration —
loop sink x in xs.into_iter() {
    // x is owned — can store in another collection
}

// — In-place mutation —
loop inout x in xs {
    x = x * 2
}

// — Adapter chains (after closures) —
val sum = xs.into_iter()
    .map(|x| x + 1)
    .filter(|x| x > 10)
    .fold(0, |a, x| a + x)

// — Map iteration —
loop (k, v) in map.into_iter() {
    print(k + ": " + v)
}

// — Range iteration —
loop i in 0..10 {
    print(i)
}

// — Still works: index-based loops —
loop i in 0..xs.count() {
    print(xs[i])
}
```

The iterator is not forced on anyone. Index-based loops remain first-class and are often
the clearest way to write a `loop inout`. The `Iterator` trait layers on top for the cases
that benefit from abstraction — chaining, laziness, and collection-agnostic code.
