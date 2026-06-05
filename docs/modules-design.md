# Modules Design — Multi-File Compilation & Imports

> **Status:** Phases 1–3 implemented and tested. Phase 4 (prelude) deferred pending stdlib.
> **Decisions baked in:** module paths use `::` (§10.1), one file = one module, `pub` visibility
> by default private (§10.3), `use` import syntax (§10.2), three-tier stdlib layering:
> `core` (auto-imported) → `collections` (explicit) → `std` (explicit, hosted).
> **Prerequisites:** none — this is foundational. Other features depend on *this*.
> **Companion docs:** [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §10 (modules), §11 (stdlib),
> [`io-design.md`](io-design.md) (the consumer — std::io depends on this),
> [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

---

## 0. The concern this answers

Today the compiler processes a single file. Every function, struct, and type lives in one
flat namespace. This is fine for v0, but it blocks everything that comes next:

- **No `core`, `collections`, or `std`.** The standard library is three tiers — without module
  support, there's nowhere to put `core::option::Option` or `collections::List`.
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

**Prelude contents** (auto-imported from `core`):
- **Types:** `Option`, `Some`, `None`, `Result`, `Ok`, `Err`
- **Primitives:** `Int`, `Float`, `Bool`, `String`, `()` (unit) — language-level, always in scope
- **Traits:** `Deinit`, `Equatable`, `Hashable`, `Ord` — core marker traits

**Not in prelude** (explicit `use` required):
- `collections::List`, `collections::Map`, `collections::Set` — collections are a capability, not vocabulary
- `io::print`, `io::println` — I/O is a side effect, visible at the import site (singular idiom)
- Everything in `std` (fs, net, math, etc.)

**Principle:** The prelude is the *language vocabulary* — types and traits so common that
requiring `use` adds noise without aiding comprehension. Collections and I/O are *capabilities*;
their imports signal what the code does. This matches Zig/Go/Mojo (builtin = language level,
everything else = library).

Prelude items have the **lowest priority** — any explicit `use` or local definition
shadows them. This prevents confusion when a user defines their own `Option` type.

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

### Phase 2 — Cross-module name resolution ✅

**Goal:** Symbols from imported modules are available in the importing module.

- [x] Extend `Resolver` to track module-level exports (`pub` items)
      — `build_global_exports()` collects pub Fn/Struct/Enum/Trait/Variant per module
- [x] Build a `GlobalSymbolTable` that spans all modules
      — `GlobalExports` type: `HashMap<String, HashMap<String, (DefId, DefKind, Visibility)>>`
- [x] Resolve `use utils::math::add` → resolver looks up `add` in `utils::math`
      — `resolve_use_path()` does multi-segment module lookup in global exports
- [x] Visibility check: non-`pub` items from other modules produce "unresolved name"
      — Only pub items are included in `GlobalExports`; private items are invisible
- [x] Alias support: `use foo::bar as b` → `b` resolves to `bar` (rename in `process_use_tree`)
- [x] Grouped imports: `use foo::{bar, baz}` works correctly

**Test:** File A defines `pub fn add(a: I32, b: I32) -> I32`. File B imports and calls it.
Type checking passes. ✅

### Phase 3 — Multi-file compilation pipeline ✅

**Goal:** The compiler driver handles parse → HIR → typeck → IR across files.

- [x] Compiler driver: discover modules → structural lowering (global ID counter) → build
      global exports → resolve with cross-module context → combine HIRs → type checking
- [x] Single IR output with all functions (qualified names already work)
- [x] `axiom run <dir>` compiles and executes multi-file projects end-to-end
- [x] Golden file tests for multi-file programs (4 test cases in `tests/fixtures/modules/`)

**Test:** Two-file program compiles end-to-end and runs in the VM. ✅

### Phase 4 — Prelude ⏸ DEFERRED

**Goal:** `Option`, `Result`, `Some`, `None`, `Ok`, `Err` available without `use`.
**Blocked on:** stdlib `core` module must exist first.

**Stdlib layout (three-tier):**

```
stdlib/
  core/                  ← auto-imported via prelude (language vocabulary)
    option.ax            — enum Option { Some(T), None }
    result.ax            — enum Result { Ok(T), Err(E) }
    iter.ax              — trait Iterator + adapters (map, filter, fold, etc.)
    string.ax            — String methods (len, contains, etc.)
    box.ax               — Box<T> (heap allocation)
    platform.ax          — extern "C" fn wrappers around libc (write, read, close, etc.)
  collections/           ← explicit import (already exists)
    list.ax              — List<T>
    map.ax               — Map<K,V>
    set.ax               — Set<T>
  io.ax                  ← explicit import (builds on core::platform)
    print, println, read_line, dbg
```

**What's auto-imported (prelude = language vocabulary):**
- Primitive types: `Int`, `Float`, `Bool`, `String`, `()` — language-level, always in scope
- Core enums: `Option`, `Some`, `None`, `Result`, `Ok`, `Err` — from `core/`
- Core traits: `Deinit`, `Equatable`, `Hashable`, `Ord`

**What requires explicit `use` (capabilities, not vocabulary):**
- `use collections::List` — collections signal data structure usage
- `use io::print` — I/O is a side effect, visible at the import site
- Everything in `std` (fs, net, math, etc.)

**Why this split:** The prelude boundary follows Zig/Go/Mojo — builtin = language level,
everything else = library. If an import helps a reader understand what capabilities the
code uses, it stays explicit. This matches the singular idiom principle: effects are visible.

- [ ] Create `core/` directory with `option.ax`, `result.ax`
- [ ] Create `prelude.ax` (compiler-internal file that re-exports from `core`)
- [ ] Compiler auto-imports prelude items into every module's name resolution scope
- [ ] Prelude items are lowest priority — explicit definitions shadow them
- [x] `extern "C" fn` syntax: lexer keywords, parser grammar, AST accessor, HIR field, IR flag, VM dispatch
- [x] `stdlib/io.ax` created with `extern "C" fn print/println`; loaded via `with_stdlib()`
- [ ] `core/platform.ax`: move extern "C" fns from io.ax to core/platform.ax (platform boundary owns extern fns)
- [ ] CLI pipeline refactor: `compile_source` must use `with_stdlib()` so HIR resolver sees io.ax definitions
- [ ] Move `print`/`println` to `io.ax` as safe wrappers around `core::platform::write` (remove from resolver/typeck/VM builtins)
- [ ] Test: `let x: Option<Int> = Some(42)` works without any `use` statement
- [ ] Test: `use collections::List` works, `List` without import does not

### Known gaps and limitations

| Gap | Impact | Status |
|---|---|---|
| **Glob imports** (`use foo::*`) | Emits `NotYetSupported` diagnostic | Deferred — explicit names preferred (singular idiom) |
| **No cycle detection** | `A imports B, B imports A` compiles without error | Needs implementation in `axiom-modules` |
| **`axiom-hir` ↔ `axiom-modules` decoupled** | HIR golden tests duplicate discovery logic instead of using `axiom-modules` | Design choice — HIR crate stays standalone; CLI bridges them |
| **No `axiom run <dir>` integration test** | End-to-end pipeline tested manually only | Should add CLI integration test |
| **No `super`/`crate` path resolution** | `use super::foo` parses but doesn't resolve | Needs resolver support |
| **Field-level visibility not enforced** | `pub struct Foo { pub x, y }` — `y` accessible across modules | Needs type checker enforcement |
| **No re-exports** (`pub use`) | Can't re-export items from submodules | Deferred |
| **CLI pipeline doesn't use `with_stdlib()`** | `axiom-cli::compile_source` parses user source directly; stdlib only loaded in `axiom-typeck::check_source_with_stdlib`. Blocks removing `print`/`println` from builtins. | Needs CLI pipeline refactor to prepend stdlib before parse+lower |

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

### Implemented (golden tests in `crates/axiom-hir/tests/fixtures/modules/`)

- **cross_module_call:** `use utils::greet` → resolves pub fn across modules ✅
- **grouped_import:** `use utils::{greet, add}` → resolves both items ✅
- **private_item_error:** importing non-pub item → "unresolved name" diagnostic ✅
- **struct_export:** `use models::Point` → resolves pub struct, struct literal works ✅

### Unit tests (`axiom-modules` crate — 10 tests)

- `flat_main_only`, `main_with_sibling_module`, `nested_directory_children`
- `multiple_sibling_modules`, `mod_ax_pattern`, `mod_ax_with_children`
- `find_by_name`, `topo_order_root_first`
- `error_missing_main`, `error_dual_module_def`

### Not yet tested

- **Circular import:** A imports B, B imports A → compile error
- **Nested modules:** `use a::b::c::fn_name` resolves correctly
- **Aliasing:** `use a::b as x` → `x::fn_name` works
- **Prelude:** `let x: Option<I32> = Some(42)` works without `use`
- **Shadowing:** local `fn Option()` shadows prelude `Option`
- **`super`/`crate` paths:** `use super::foo` resolves correctly

---

## 7. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Scope creep (packages, registries) | Blocks everything | Strict boundary: file-based modules only. No `forge.toml`, no packages. |
| Performance of multi-file compilation | Slow compiles | Parse files lazily (only on import), cache parsed modules in memory |
| Prelude confusion | Subtle shadowing bugs | Lowest priority — any explicit definition wins. Document clearly. |
| `core` ↔ `std` circular deps | Compiler stdlib broken | Strict layering: `core` never imports from `std`. Enforced at compile time. |
