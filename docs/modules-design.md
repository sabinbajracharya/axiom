# Modules Design — Multi-File Compilation & Imports

> **Status:** design phase. Not yet implemented. Binding before code is written.
> **Decisions baked in:** module paths use `::` (§10.1), one file = one module, `pub` visibility
> by default private (§10.3), `use` import syntax (§10.2), `core`/`std` two-tier stdlib
> layering (§11).
> **Prerequisites:** none — this is foundational. Other features depend on *this*.
> **Companion docs:** [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §10 (modules), §11 (stdlib),
> [`io-design.md`](io-design.md) (the consumer — std::io depends on this),
> [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

---

## 0. The concern this answers

Today the compiler processes a single file. Every function, struct, and type lives in one
flat namespace. This is fine for v0, but it blocks everything that comes next:

- **No `core` or `std`.** The standard library is a collection of modules — without module
  support, there's nowhere to put `core::option::Option` or `std::io::println`.
- **No imports.** `use std::io::print` doesn't parse, resolve, or compile.
- **No multi-file programs.** Real programs split code across files. One file = one program
  is a prototype constraint, not a language feature.
- **No traits in `core`.** `Iterator`, `Display`, `Eq` all belong in `core` — but without
  modules, there's no `core`.
- **No visibility control.** Everything is equally accessible. `pub` has no meaning without
  module boundaries.

The module system is the prerequisite for the standard library, traits, collections, and
every user program larger than a single file.

---

## 1. The design, stated plainly

### 1.1 What's already decided (DESIGN_SPEC §10)

- **One file = one module.** Directory structure maps to the module tree.
- **`mod name { ... }`** for in-file submodules (rarely used, but supported).
- **Paths use `::`** — `std::io::print`, `mymod::helper`. Field/method access uses `.`.
  (Same split as Oxy.)
- **Import syntax:**
  ```
  use std::io::print
  use std::collections::{Map, Set}
  use mymod::helper as h
  ```
- **Visibility:** everything private by default. `pub` exports from a module.
- **Glob imports** (`use foo::*`) are discouraged/lint-warned (singular idiom: explicit
  names aid the reader). Available but not idiomatic.

### 1.2 What needs designing

#### 1.2.1 File layout

```
project/
  src/
    main.ax            ← entry point (root module)
    utils.ax           ← `use utils::helper`
    utils/
      math.ax          ← `use utils::math::add`
  core/                ← compiler-provided, read-only
    option.ax
    result.ax
    iter.ax            ← Iterator trait
    io.ax              ← Writer trait (after extern-fn-design.md)
    ...
  std/                 ← compiler-provided, read-only
    io.ax              ← print, println
    string.ax          ← string::format
    ...
```

- `core/` and `std/` are **compiler-provided search paths** — always available, ship with
  the compiler, not part of the user's project.
- User modules resolve relative to `src/` (or the entry file's directory).
- No `forge.toml` needed yet — package management is deferred (§10.4).

#### 1.2.2 Import resolution algorithm

```
resolve("std::io::print"):
  1. "std" matches a compiler search path → enter std/
  2. Find io.ax → parse it
  3. Find `pub fn print` → return DefId + type info
  4. Error if not found or not pub

resolve("utils::math::add"):
  1. "utils" doesn't match a search path → resolve relative to src/
  2. Find utils/math.ax or utils.ax + mod math → parse it
  3. Find `pub fn add` → return DefId + type info

resolve("helper"):
  1. No `::` — look in current module first
  2. Then look in prelude
  3. Error if ambiguous or not found
```

#### 1.2.3 Prelude

A small set of names available in every module without `use`. The prelude is a
compiler-internal mechanism: during name resolution, prelude items are treated as if every
module has an implicit `use prelude::*`.

**Prelude contents** (auto-imported):
- `Option`, `Some`, `None` — from `core::option`
- `Result`, `Ok`, `Err` — from `core::result`

Prelude items have the **lowest priority** — any explicit `use` or local definition
shadows them. This prevents confusion when a user defines their own `Option` type.

The prelude is small. `print`/`println` join it later (after the std::io work), but the
prelude mechanism itself is built here.

#### 1.2.4 Multi-file compilation model

Today the compiler processes one file. With modules it must:

1. **Parse** the entry file → discover `use` statements → recursively parse imported files.
2. **Build a module graph** — a DAG of modules and their imports. Cycle detection = error.
3. **HIR lowering** happens per-module, with cross-module symbol resolution.
4. **Type checking** must see all modules (cross-module type references).
5. **IR lowering** emits one IR unit with all functions. Qualified names (`Point::dist`)
   already work — no IR changes needed.

#### 1.2.5 Visibility enforcement

- `pub fn add(...)` — visible from any module that imports it.
- `fn add(...)` — visible only within the defining module.
- `pub struct Foo { pub x: I32, y: I32 }` — `Foo` and `x` are visible; `y` is not.
- Visibility is checked during **name resolution** (can I see this symbol?) and during
  **type checking** (can I access this field?).

---

## 2. What this does NOT include

| Feature | Status | Why |
|---|---|---|
| `forge.toml` / packages | `[Deferred → own milestone]` | §10.4 — package management is separate from module resolution |
| `forge add <pkg>` / registries | `[Deferred → own milestone]` | Supply chain tooling, not language feature |
| `mod name { ... }` inline submodules | `[Deferred → when needed]` | File-based modules are sufficient for v1; inline submodules are convenience |
| Circular module dependencies | `[Not allowed]` | DAG — detected at compile time, clear error message |
| Conditional compilation (`#[cfg]`) | `[Deferred → v2]` | Platform-specific code — not needed yet |
| Module-level constants | `[Deferred → with const]` | `const` is not yet in the language |
| Re-exports (`pub use`) | `[Deferred → when needed]` | Convenience — not required for core/std |

---

## 3. Implementation phases

### Phase 1 — Module graph & file discovery ✅

**Goal:** The compiler can process multiple `.ax` files and understand `use` statements.

- [x] Add `ModuleGraph` struct — maps module paths to file paths (`axiom-modules` crate)
- [x] File discovery: `main.ax` entry point, `foo.ax` + `foo/` siblings, `mod.ax` pattern
- [ ] Add compiler search paths for `core/` and `std/` directories
- [x] Parse `use std::io::print` → produce `UseItem` HIR node (lowered from `UseDecl`)
- [x] Parse `use std::collections::{Map, Set}` → multi-import (`UseTreeKind::Group`)
- [x] Parse `use foo::bar as h` → aliased import (`rename` in `UseTreeKind::Single`)
- [ ] Error: circular imports (not yet — no cycle detection in module graph)
- [x] Error: module not found (`ModuleError` variants)
- [x] Error: duplicate import (via `DuplicateDefinition` diagnostic)

**Test:** Two user files, one imports from the other. Import resolution succeeds. ✅
**Note:** `super`/`crate` keywords added to lexer and parser.

### Phase 2 — Cross-module name resolution 🔶 PARTIAL

**Goal:** Symbols from imported modules are available in the importing module.

- [ ] Extend `Resolver` to track module-level exports (`pub` items)
- [ ] Build a `GlobalSymbolTable` that spans all modules
- [ ] Resolve `use utils::math::add` → resolver looks up `add` in `utils::math`
      **Status: BROKEN.** `resolve_use_path` only searches the current module's
      `top_level` scope. Cross-module `use` items produce "unresolved name" errors.
- [ ] Visibility check: error if accessing non-`pub` item from another module
      **Status: NOT DONE.** Non-pub items give "unresolved name" instead of a
      proper visibility error.
- [x] Alias support: `use foo::bar as b` → `b` resolves to `bar` (rename in `process_use_tree`)

**Test:** File A defines `pub fn add(a: I32, b: I32) -> I32`. File B imports and calls it.
Type checking passes. ❌ **FAILS** — cross-module resolution not wired up.

### Phase 3 — Multi-file compilation pipeline 🔶 PARTIAL

**Goal:** The compiler driver handles parse → HIR → typeck → IR across files.

- [x] Compiler driver: discover modules → parse all files → build module graph → HIR
      lowering (per-module) → combine HIRs → type checking combined HIR
- [x] Single IR output with all functions (qualified names already work)
- [ ] Golden file tests for multi-file programs

**Test:** Two-file program compiles end-to-end and runs in the VM. ❌ **FAILS**
(depends on Phase 2 cross-module resolution working)

### Phase 4 — Prelude ⏸ DEFERRED

**Goal:** `Option`, `Result`, `Some`, `None`, `Ok`, `Err` available without `use`.
**Blocked on:** stdlib `core` module must exist first.

- [ ] Create `prelude.ax` (compiler-internal file that re-exports from `core`)
- [ ] Compiler auto-imports prelude items into every module's name resolution scope
- [ ] Prelude items are lowest priority — explicit definitions shadow them
- [ ] Test: `let x: Option<I32> = Some(42)` works without any `use` statement

---

## 4. Dependency graph

```
Phase 1 (module graph)
    │
    ▼
Phase 2 (cross-module name resolution)
    │
    ▼
Phase 3 (multi-file pipeline)
    │
    ▼
Phase 4 (prelude)
```

Each phase builds on the previous. Phases are self-contained PRs.

---

## 5. Compiler architecture changes

### 5.1 New or extended crate: `axiom-modules` (or extend `axiom-driver`)

Responsible for:
- Module graph construction
- File discovery and search paths
- Import resolution
- Multi-file orchestration of the compilation pipeline

### 5.2 Changes to existing crates

| Crate | Change |
|---|---|
| `axiom-parser` | Parse `use` statements into `Import` HIR nodes |
| `axiom-hir` | Add `Item::Use { path, alias, items }` node |
| `axiom-resolve` (or extend `axiom-typeck`) | Cross-module symbol table, visibility checking |
| `axiom-driver` | Multi-file compilation, search paths, prelude injection |

### 5.3 No VM changes

The module system is entirely a compile-time concern. The VM sees flat IR with qualified
function names — it doesn't know about modules, imports, or visibility. This is by design:
modules affect *how* code is compiled, not *how* it runs.

---

## 6. Testing strategy

- **Two-file program:** file A defines `pub fn`, file B imports and calls it
- **Visibility:** file B can't access non-`pub` item from file A → compile error
- **Circular import:** A imports B, B imports A → compile error
- **Nested modules:** `use a::b::c::fn_name` resolves correctly
- **Aliasing:** `use a::b as x` → `x::fn_name` works
- **Prelude:** `let x: Option<I32> = Some(42)` works without `use`
- **Shadowing:** local `fn Option()` shadows prelude `Option`

---

## 7. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Scope creep (packages, registries) | Blocks everything | Strict boundary: file-based modules only. No `forge.toml`, no packages. |
| Performance of multi-file compilation | Slow compiles | Parse files lazily (only on import), cache parsed modules in memory |
| Prelude confusion | Subtle shadowing bugs | Lowest priority — any explicit definition wins. Document clearly. |
| `core` ↔ `std` circular deps | Compiler stdlib broken | Strict layering: `core` never imports from `std`. Enforced at compile time. |
