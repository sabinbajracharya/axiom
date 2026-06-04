# Struct & Collection Library-Type Plan — v0

> **Goal:** Migrate `List<T>` and `Map<K,V>` from compiler built-ins to library types
> backed by user-defined structs. This retires audit item #3 (built-in collections) and
> moves the v1 "library type" design to v0.
>
> **Status:** Plan created. Steps checked off as completed.

---

## What already works (confirmed by tests)

- [x] **Parser:** `StructDef`, `StructLitExpr`, `StructLitFieldList`, `StructLitField` syntax kinds
- [x] **HIR:** `StructDef` lowering (fields, type params, visibility), `ImplDef` lowering
- [x] **Type checker:** `Ty::Struct(StructTy)`, `StructInfo`, `collect_struct_defs`, field type registration, struct literal inference, field access (`p.x`), `impl` block collection, `Deinit` auto-trait
- [x] **Tests pass:** `test_golden_simple_struct`, `test_golden_structs`, `test_golden_struct_field_access`, `test_golden_structs_enums_match`

---

## What's missing for collections-as-library-types

### Step 1: `HeapBuffer<T>` IR primitive ✅

The compiler needs a raw heap allocation primitive that library collections build on.
`DynamicBuffer<Header, Element>` from the design doc is the full version — for v0 we
can start with a simpler `HeapBuffer<T>` that just allocates/deallocates a contiguous
block of `T` elements.

**Files to touch:**
- `crates/axiom-ir/src/ir.rs` — add `IrInstr::HeapAlloc`, `IrInstr::HeapFree`, `IrInstr::HeapGet`, `IrInstr::HeapSet`
- `crates/axiom-typeck/src/types.rs` — add `Ty::HeapBuffer(Box<Ty>)` (or use `Ty::Nominal` if we define it as a struct)
- `crates/axiom-typeck/src/typeck/builtin.rs` — register `HeapBuffer<T>` as a builtin type
- `docs/ir-design.md` — document the primitive

**Exit gate:** `HeapBuffer<Int>` can be allocated, indexed, and freed in IR.

---

### Step 2: `Deinit` auto-impl for user-defined structs ✅

`Deinit` auto-impls exist for primitives (Int, Float, Bool, String, Unit). Need to extend
so user-defined structs also get `Deinit` automatically — a struct's `drop` calls `drop`
on each field.

**Files to touch:**
- `crates/axiom-typeck/src/typeck/collect.rs` — added `register_struct_deinit_impls` called after `collect_struct_defs`, registers Deinit impl for each user-defined struct
- `crates/axiom-typeck/src/typeck/infer.rs` — removed hardcoded `if bound == "Deinit" { return; }` shortcut; Deinit now resolved via impl table like all other traits
- `crates/axiom-typeck/tests/builtin_traits.rs` — added `test_deinit_bound_satisfied_for_nested_struct`

**Exit gate:** `struct Foo { x: Int }` satisfies `T: Deinit`. ✅

---

### Step 3: Subscript declarations (`yield`)

Collections need `xs[i]` syntax. The design specifies subscript declarations with `yield`
that suspend/resume for in-place mutation. For v0, we can start with read-only subscripts
and defer `inout` projections.

**Parser:**
- `crates/axiom-parser/src/syntax_kind.rs` — add `SubscriptDecl`, `YieldStmt`
- `crates/axiom-parser/src/parser.rs` — parse `subscript(let self, i: Int) -> T { yield expr }`

**HIR:**
- `crates/axiom-hir/src/hir/mod.rs` — add `SubscriptDef` node
- `crates/axiom-hir/src/lower/item.rs` — lower `SubscriptDef` from AST

**Type checker:**
- `crates/axiom-typeck/src/typeck/methods.rs` — resolve `xs[i]` as subscript call on the receiver type
- `crates/axiom-typeck/src/typeck/collect.rs` — collect subscript definitions from impl blocks

**Exit gate:** `xs[0]` on a struct with a subscript definition resolves and type-checks.

---

### Step 4: Generic struct method resolution

`impl<T> List<T> { fn push(inout self, sink element: T) }` needs to work — when calling
`my_list.push(42)`, the type checker must substitute `T = Int` in the method signature.

**Files to touch:**
- `crates/axiom-typeck/src/typeck/methods.rs` — extend `find_impl_method` to match on `Instance` types (name + args), substitute type params in method signature
- `crates/axiom-typeck/src/typeck/collect.rs` — collect `impl<T>` blocks and register methods with type param placeholders

**Exit gate:** `impl<T> Foo<T> { fn get(let self) -> T }` resolves correctly on `Foo<Int>`.

---

### Step 5: Migrate `List<T>` to library type

Rewrite `List<T>` as an Axiom struct backed by `HeapBuffer<T>`, defined in a standard
library file. Remove `builtin_types` registry entry for "List".

**Files to touch:**
- New file: `stdlib/collections/list.ax` — `struct List<T: Deinit> { ... }` with `push`, `count`, `is_empty`, `capacity`, subscript
- `crates/axiom-typeck/src/typeck/builtin.rs` — remove "List" from `builtin_types`, remove hardcoded List methods
- `crates/axiom-typeck/src/typeck/collect.rs` — load stdlib structs into the type checker
- `crates/axiom-typeck/tests/collections.rs` — update tests to use library List

**Exit gate:** `val xs: List<Int> = [1, 2, 3]` works with `List` defined as a library struct, not a compiler built-in.

---

### Step 6: Migrate `Map<K,V>` to library type

Same as Step 5 but for `Map<K,V>`. Requires `Hashable` + `Equatable` trait bounds.

**Files to touch:**
- New file: `stdlib/collections/map.ax` — `struct Map<K: Hashable + Equatable, V: Deinit> { ... }`
- `crates/axiom-typeck/src/typeck/builtin.rs` — remove "Map" from `builtin_types`
- `crates/axiom-typeck/tests/collections.rs` — update tests

**Exit gate:** `val m: Map<String, Int> = ["a": 1]` works with library Map.

---

### Step 7: Cleanup — remove built-in collection infrastructure

Remove all compiler-internal collection machinery that's been replaced by library types.

**Files to touch:**
- `crates/axiom-typeck/src/typeck/builtin.rs` — remove `builtin_types` HashMap, collection-specific method registration
- `crates/axiom-typeck/src/typeck/methods.rs` — remove collection-specific special cases
- `docs/design-audit-m1m2.md` — mark item #3 as DONE
- `docs/collection-type-design.md` — update §7 migration path (v0 done, not v2)

**Exit gate:** No `List` or `Map` strings in the type checker source. All collection behavior comes from library code.

---

## Dependency order

```
Step 1 (HeapBuffer)  ──┐
Step 2 (Deinit)      ──┼──→ Step 5 (List library) ──→ Step 7 (cleanup)
Step 3 (Subscripts)  ──┤                          ──→ Step 6 (Map library)
Step 4 (Generic methods)┘
```

Steps 1–4 can be worked in parallel. Steps 5–6 depend on all of 1–4. Step 7 is cleanup.

---

## Design decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | `HeapBuffer<T>` not `DynamicBuffer<Header, Element>` for v0 | Simpler — header/element split is a Perceus optimization, defer to v1 |
| 2 | Read-only subscripts for v0, `inout` deferred | `yield inout` needs suspend/resume semantics — complex, defer |
| 3 | `Deinit` auto-impl generated at collect time | Same pattern as Equatable/Hashable/Ord auto-impls already in builtin.rs |
| 4 | stdlib files loaded by the type checker, not the parser | Keeps the parser collection-agnostic; type checker reads `.ax` stdlib at startup |
| 5 | `List<T>` uses `HeapBuffer<T>` directly, not `DynamicBuffer<Int, T>` | v0 simplification — header fields (count, cap) live in the List struct itself |
