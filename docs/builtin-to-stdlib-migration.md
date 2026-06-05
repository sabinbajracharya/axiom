# Migrating Compiler Built-ins → `stdlib` (`core`)

> **Status:** Planned — not started. This is the umbrella plan for retiring every
> compiler-baked built-in that is *expressible in Axiom* and moving it into real
> `stdlib/core/*.ax` code. It is the same migration `print`/`println` already
> completed (see [`string-format-and-print-retire.md`](string-format-and-print-retire.md))
> applied to the rest of the stand-ins.
>
> **Companion docs:** [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §11 (stdlib surface),
> §14 (roadmap — "migration of `List`/`Map` from compiler built-ins to library types"),
> [`io-design.md`](io-design.md), [`struct-v0-plan.md`](struct-v0-plan.md)
> (`HeapBuffer<T>` — the collections prerequisite),
> [`extern-buffers-and-path-unification.md`](extern-buffers-and-path-unification.md).

---

## 1. The dividing line (the rule this plan applies)

A symbol is a **legitimate, permanent built-in / intrinsic** *only if it cannot be
written in Axiom*. Exactly two things qualify:

1. **It bottoms out at the platform / VM representation** — e.g. `String::as_bytes`
   (the `String`→`Bytes` primitive), `core::platform` externs (`write`/`read`/`close`),
   raw scalar equality/ordering/hash, the growable backing store.
2. **It needs syntax the language does not have (and won't add)** — `format`, because
   Axiom has no varargs and the singular idiom forbids adding it. This is "the only
   magic call."

**Everything else that is expressible in Axiom must be ordinary library code.** A
built-in for an expressible operation is a *stand-in* — a placeholder for stdlib code
that isn't written yet. `print` was exactly such a stand-in; the ones below are the
remainder.

---

## 1a. Decided folder structure [Decided]

The physical `stdlib/` tree **mirrors the module path** (one file = one module; the
module name is the relative path, so `stdlib/core/string.ax` → `core::string`). The
`std::`/`core::` prefixes from `DESIGN_SPEC.md` §10.2/§11 are kept. Target layout:

```
stdlib/
  core/                      # always-available prelude (core::*)
    traits.ax        -> core::traits       # Deinit, Equatable, Hashable, Ord (+ Iterator later)
    primitives.ax    -> core::primitives   # impl Int/Float/Bool methods + their trait impls
    string.ax        -> core::string       # String type + methods (len; as_bytes is the floor)
    platform.ax      -> core::platform     # internal extern boundary (EXISTS; users never import)
    option.ax        -> core::option       # Option         (later)
    result.ax        -> core::result       # Result         (later)
  std/                       # hosted (std::*)
    io.ax            -> std::io            # print, println, read_line, dbg   (MOVE from stdlib/io.ax)
    string.ax        -> std::string        # `format` surface (thin/none — format is an intrinsic)
    collections/
      list.ax        -> std::collections::list   # List<T>   (MOVE from stdlib/collections/list.ax)
      map.ax         -> std::collections::map     # Map<K,V>  (MOVE from stdlib/collections/map.ax)
      set.ax         -> std::collections::set     # Set        (later)
```

**Decisions captured** (spec §11 updated in the same change):
- **Collections → `std::collections`, not `core`** — they need a heap allocator; `core`
  stays the minimal always-on layer. Resolves the §10.2-vs-§11 contradiction toward §10.2.
- **`String` is the one `core` exception that allocates** — it is the sole string type
  (no `&str` split), backs literals + `format`, so it must always be available.
- **`io` moves under `std`** (`std::io`) to match `use std::io::print`; the current
  top-level `stdlib/io.ax` is relocated.

> **Relocation is prerequisite P0 below** — moving the existing files changes their module
> paths, the `with_stdlib` concatenation, and every `use`/test that references them.

---

## 2. Full inventory (current state)

### 2a. STAY — legitimate intrinsics (do **not** migrate)

| Symbol | Layer | Why it stays |
|---|---|---|
| `Int` `Float` `Bool` `String` `Unit` (`Bytes`) | HIR `builtin_def_id` / type names | language primitives |
| `format` | HIR + typeck `infer_call` + VM `is_builtin` | the one variadic primitive (no varargs syntax) |
| `todo` | HIR + typeck `builtin_fn` | compiler stub, like Rust's `todo!()` |
| `write` `read` `close` | VM `PlatformFn` / `core::platform` | the OS boundary (extern "C") |
| `String::as_bytes` | VM `is_builtin` + typeck method | irreducible `String`→`Bytes` primitive |
| raw scalar `==` `<` `+` … | VM `BinOp` (already operators, not builtins) | primitive floor |

### 2b. MIGRATE — stand-ins baked into the compiler (the work of this plan)

| # | Stand-in | Where it lives now | Target home | Bottoms out on |
|---|---|---|---|---|
| M1 | trait **declarations** `Deinit` `Equatable` `Hashable` `Ord` | `typeck/builtin.rs::register_builtin_traits` | `stdlib/core/traits.ax` (`core::traits`) | nothing (pure decls) |
| M2 | `Deinit` auto-impl for all 5 types | `register_builtin_impls` | `core/primitives.ax` + `core/string.ax` (empty bodies) | nothing |
| M3 | `Equatable`/`Ord` auto-impls for the 4 primitives | `register_builtin_impls` | `core/primitives.ax` + `core/string.ax`, using `==`/`<` | scalar `==`/`<` (operators) |
| M4 | `Hashable` auto-impl for the 4 primitives | `register_builtin_impls` | `core/primitives.ax` + `core/string.ax` | a tiny `hash` scalar intrinsic |
| M5 | `String::len` | `register_string_methods` + VM `is_builtin` | `stdlib/core/string.ax` (`self.as_bytes()…`) | `Bytes` length (floor) |
| M6 | `List::push` (+ `new`/`count`/`is_empty`/`capacity`/subscript stubs) | `register_list_methods` + `collections/list.ax` (`todo()` bodies) | `stdlib/std/collections/list.ax` (real bodies) | `HeapBuffer<T>` primitive |
| M7 | `Map::set` (+ `get`/`has`/`count`/… stubs) | `register_map_methods` + `collections/map.ax` (`todo()` bodies) | `stdlib/std/collections/map.ax` (real bodies) | `HeapBuffer<T>` + `Hashable` |

> Note: M3 is writable in Axiom **today** because primitive `==`/`<` are already VM
> operators, not builtins — so `fn eq(let self, other: Int) -> Bool { self == other }`
> needs no new intrinsic. M4 (hash) and M5/M6/M7 each need one new irreducible floor
> primitive, called out per phase below.

---

## 3. Prerequisites (must land before/with the migration)

- [ ] **P0 — Relocate existing stdlib files to the §1a layout.** `git mv stdlib/io.ax
      stdlib/std/io.ax`; `git mv stdlib/collections/ stdlib/std/collections/`. Update the
      `with_stdlib` concatenation list, `discover_library` walk (already path-relative), every
      `use`/path that names `io::`/`collections::`, and the fixtures/goldens that reference
      them. Module paths become `std::io` / `std::collections::*`. Regen goldens; gate green.
      *(Pure move + path-rename; no semantics change. Do this first so later phases target the
      final paths.)*
- [x] **P1 — `core` bodies load and type-check in *every* path.** ✅ Done by
      [`stdlib-loading-unification.md`](stdlib-loading-unification.md): the four divergent
      loaders are collapsed into one. The stdlib is **embedded** (`axiom-stdlib` build.rs) and
      every path — single-file, project dir, tests — compiles through the one
      `axiom_typeck::check_modules` pipeline **with bodies**. There is no longer an
      exports-only/no-bodies path. Any new `core/*.ax` file is auto-embedded (drift-guarded)
      and loaded everywhere; just add it to the implicit prelude if it should resolve without
      `use`.
- [ ] **P2 — core test/golden harness asserts clean.** Extend the VM golden + typeck
      harness (already tightened to assert `thir.diagnostics.is_empty()`) to cover the
      `core` modules, so a broken `core/*.ax` fails the build instead of silently falling
      back to a built-in.
- [ ] **P3 — minimal scalar floor primitives identified & named.** Confirm `==`/`<`/
      arithmetic are VM operators (they are). Add the *one* new floor op each later phase
      needs: a per-scalar `hash` (for M4) and a `Bytes` length op (for M5). Keep these as
      named VM intrinsics behind the same `is_builtin` door — they are the irreducible
      floor, not stand-ins.
- [x] **P4 — `HeapBuffer<T>` growable primitive + subscript** (gates M6/M7). Done in **D1**:
      the heap-backed growable buffer with the four floor ops + subscript read/write, exposed
      to Axiom. (`Deinit`/refcounting is Perceus/v1 territory; the caller tracks length, as
      `List` will via its `count`/`cap` fields.) List/Map can now be written in Axiom.
- [ ] **P5 — decision: explicit per-primitive impls vs. a `derive`.** The current
      auto-impl table generates `4×{Equatable,Hashable,Ord}` + `5×Deinit` impls. Decide
      whether `core` writes these out explicitly (simplest; no new machinery; singular-idiom
      friendly) or whether we introduce a `derive` mechanism first. **Default: explicit
      impls** — defer `derive` until duplication actually hurts.

---

## 4. Implementation plan (ordered; each step ≈ one commit; TDD; gate must pass)

### Phase A — trait declarations (lowest risk, no new primitives) ✅ DONE (M1)
- [x] **A0 (added).** Implement real **supertrait syntax** `trait X: A + B { .. }`
      end-to-end (parser → AST → HIR `TraitDef.supertraits` → typeck `collect_trait_defs`),
      documented in `DESIGN_SPEC.md` §3.5. Needed because the four traits use
      `Hashable: Equatable` / `Ord: Equatable` and the parser had no supertrait syntax (it
      was hand-faked in the old registry). Proper impl, not a shim.
- [x] **A1.** Created `stdlib/core/traits.ax` with `trait Deinit`, `Equatable`,
      `Hashable: Equatable`, `Ord: Equatable`, with **proper** signatures
      (`eq`/`lt` take `other: Self`; `drop`/`hash` take `let self`) — not the empty-param
      registry stubs. Auto-embedded by `axiom-stdlib`; collected via `collect_trait_defs`.
- [x] **A2.** Deleted `register_builtin_traits` + its `collect_pass` call + its unit tests;
      trait resolution now finds them from `core::traits`. Migrated `builtin_traits.rs` to the
      stdlib path (bare mode has no stdlib by design). Regenerated multi-file HIR goldens.
      Gate green.

### Phase B — primitive trait impls (in `core/primitives.ax` + `core/string.ax`) ✅ DONE (M2/M3/M4)
- [x] **B0 (added).** Enable trait impls on builtin primitive types: `collect_impl_defs`
      recognized impl targets only via `env.lookup`, so `impl Trait for Int` errored
      (`TypeNotFoundForImpl`). Recognize the builtin primitive names; verified `impl …
      for Int` type-checks, satisfies a bound, and runs.
- [x] **B1.** `impl Deinit for {Int,Float,Bool,Unit}` (core/primitives.ax) + `String`
      (core/string.ax). Removed the Deinit rows + `ALL_TYPES`. Made the core traits `pub`
      and added `core::traits` to the implicit prelude so trait names resolve in impls/bounds.
      Regen. *(M2)*
- [x] **B2.** `impl Equatable` (body `self == other`) and `impl Ord` (body `self < other`)
      for the 4 primitives. Removed those rows. Added core_traits_e2e. Regen. *(M3)*
- [x] **B3.** Added the `hash_raw` scalar floor (typeck method + VM deterministic hash);
      `impl Hashable for {…}` forwards to it. Removed the Hashable rows — `register_builtin_impls`
      is now gone entirely. Fixed `resolve_impl_self_type` for primitive impls (was `Ty::Error`).
      Regen + e2e. *(M4)*

### Phase C — `String::len` → library
- [ ] **C1.** Add the `Bytes` length floor op (P3). Write `impl String { fn len(let self)
      -> Int { … self.as_bytes() … } }` in `stdlib/core/string.ax`. Remove `len` from
      `register_string_methods` **and** from VM `is_builtin`/`call_builtin`
      (`builtin_string_len`). `as_bytes` stays (it's the floor). Regen + `format_e2e`-style
      e2e test. *(M5)*

### Phase D — collections (largest; gated on P4 = `HeapBuffer<T>`)
- [x] **D1.** Landed `HeapBuffer<T>` (P4): the four floor ops are exposed to Axiom as
      compiler intrinsics — `heap_alloc<T>(count) -> [T]`, `heap_get<T>([T], i) -> T`,
      `heap_set<T>([T], i, T)`, `heap_free<T>([T])` — lowering to the VM's
      `HeapAlloc`/`HeapGet`/`HeapSet`/`HeapFree` instructions. `[T]` slice syntax resolves to
      `Ty::HeapBuffer`; `buf[i]` (read) and `buf[i] = v` (`IndexSet`) work on buffers.
      `heap_alloc`'s **return-only** type parameter is bound from the binding annotation via
      unification (generalised `val`/`var` annotation check from `==` to `unify`); added
      `Ty::HeapBuffer` arms to `unify`/`substitute`/`contains_type_param`. Tests:
      `axiom-typeck/tests/heap_buffer.rs` (incl. negatives) + `axiom-vm/tests/heap_buffer_e2e.rs`.
- [x] **D2.** Implemented real `stdlib/std/collections/list.ax`: `struct List<T> { buf: [T],
      count, cap }` with `new`/`count`/`is_empty`/`capacity`/`push`/`grow`/subscript bodies on
      the `HeapBuffer<T>` floor — `push` doubles the buffer when full. Removed
      `register_list_methods` (incl. the `push` intrinsic) and its unit test. Added
      `axiom-vm/tests/list_e2e.rs` (push/count/subscript, growth across boundaries, is_empty).
      Regenerated multi-file HIR goldens. *(M6)*
      Unblocking work landed alongside: **associated-function calls** (`List::new()`) now
      resolve end-to-end — an additive `CallExpr.qualifier` (the path before the last segment;
      enum constructors and module-qualified calls are untouched), typeck resolution of
      associated fns in the impl's type-param scope, and IR qualification to `Type::method`;
      `check_expr` adopts the expected type when the inferred one unifies modulo type
      parameters (so `List::new`'s phantom `T` binds from the declared `List<Int>`). The IR
      assignment-lowering cluster moved to `crates/axiom-ir/src/lower/assign.rs` (600-line cap).
- [ ] **D3.** Implement real `stdlib/std/collections/map.ax` bodies on `HeapBuffer` +
      `Hashable` (needs B3). Remove `register_map_methods` (incl. the `set` intrinsic). Add
      map e2e tests. Regen. *(M7)*

### Phase E — cleanup & docs
- [ ] **E1.** `typeck/builtin.rs` now holds *only* the irreducible floor (or is deleted if
      the floor ops live elsewhere). Confirm no `register_builtin_*` stand-in remains.
- [ ] **E2.** Update `DESIGN_SPEC.md` §11 (mark `core` primitive methods / traits as real
      library code), §14 roadmap row, and this doc's status. Update per-folder READMEs.
- [ ] **E3.** Final inventory check: the only built-ins left are the §2a "STAY" set
      (`format`, `todo`, primitive type names, platform externs, `as_bytes`, scalar floor).

---

## 5. Out of scope (deferred, with reason)

| Item | Why deferred |
|---|---|
| `derive` mechanism | Explicit `core` impls suffice (P5); add `derive` only when duplication hurts. |
| `Display`/`Debug` user-type dispatch | Tracked in `string-format-and-print-retire.md` §6 (trait-object story). |
| Native-backend versions of the floor ops | Same VM-callback → real-FFI path as the rest of `core::platform` (io-design.md). |
| Removing `as_bytes` / scalar floor | Irreducible by definition (§1) — they are not stand-ins. |
