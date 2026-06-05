# std::io Design — Writer Trait, Extern Fn & Removing Builtins

> **Status:** Layers 1–2 implemented. `extern "C" fn` syntax works through
> lexer→parser→HIR→IR→VM. `stdlib/io.ax` has real `pub fn print`/`println` that
> call `core::platform::write_string`/`write_line`. VM dispatches extern fns via
> builtin table (no real FFI). Both single-file and multi-file paths resolve
> `print`/`println` through the stdlib module system — no hardcoded builtins in
> resolver or type checker.
>
> **Architecture:** Two layers following Go/Rust/Zig — `core::platform` owns the unsafe
> platform boundary (extern "C" fns around libc), `std::io` builds safe user-facing APIs
> on top. Users import `std::io`, never `core::platform` directly.
>
> **Decisions baked in:** `core`/`std` two-tier stdlib layering (§11),
> `Display`/`Debug` traits for formatting (§11), `string::format` as the one formatting
> mechanism (§11), `std::io` includes `print`, `println`, `read_line`, `dbg` (§11).
> **Prerequisites:**
> - [`modules-design.md`](modules-design.md) — Phase 1–3 (module graph, cross-module name
>   resolution, multi-file pipeline). Needed to host `core::io` and `std::io` as modules.
> - [`modules-design.md`](modules-design.md) — Phase 4 (prelude). Needed for `print`/`println`
>   auto-import.
> **Companion docs:** [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §11 (stdlib),
> [`modules-design.md`](modules-design.md) (the prerequisite — modules),
> [`traits-design.md`](traits-design.md) (Writer is a trait),
> [`methods-design.md`](methods-design.md) (method dispatch on Writer impls),
> [`ir-design.md`](ir-design.md) (ExternCall instruction),
> [`vm-design.md`](vm-design.md) (host callback mechanism),
> [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

---

## 0a. Implementation status

| Component | Status | Notes |
|---|---|---|
| `extern "C" fn` lexer keywords | ✅ Done | `extern` and `unsafe` keywords in token.rs, symbols.rs, syntax_kind.rs |
| `extern "C" fn` parser grammar | ✅ Done | `item()` and `member_list()` consume `extern` + optional ABI string before `fn_def` |
| `extern_abi()` AST accessor | ✅ Done | `FnDef.extern_abi()` returns `Some("C")`, `Some("")`, or `None` |
| `extern_abi` HIR field | ✅ Done | `FnDef.extern_abi: Option<String>` — set during lowering |
| `IrFunction.is_extern` | ✅ Done | Field added, set from `extern_abi` during IR lowering |
| VM extern dispatch | ✅ Done | Extern fns dispatched via `call_builtin()` (same as hardcoded builtins) |
| `stdlib/io.ax` | ✅ Done | Real `pub fn print`/`println` calling `core::platform::write_string`/`write_line` |
| CLI loads stdlib via module system | ✅ Done | Multi-file path: `discover_library()` + `merge()` |
| `core/platform.ax` | ✅ Done | `pub extern "C" fn write_string`/`write_line` + raw `write`/`read`/`close` |
| Remove `print`/`println` builtins from resolver | ✅ Done | Both paths resolve via stdlib module system; no hardcoded builtins in resolver/typeck |
| Safe wrappers (`pub fn print`/`println`) | ✅ Done | `io::print`/`println` are real Axiom functions calling `core::platform` externs |
| Real FFI (`dlsym`/`libloading`) | ❌ Deferred | Needs Cranelift JIT backend; VM uses builtin dispatch table |
| `unsafe` blocks | ❌ Deferred | Keywords exist, grammar not implemented |

---

## 0b. The concern this answers

Today, `print` and `println` are VM builtins — hardcoded strings intercepted in the VM's
call dispatch. This is the correct v0 approach, but it's a dead end:

- **No composability.** Can't write to a file, a buffer, or a socket with the same API.
  Every I/O target needs its own builtin.
- **No testability.** Can't capture output in tests without intercepting stdout at the OS
  level.
- **No principled I/O.** The design spec (§11) says `std` includes `io` backed by
  `Writer`/`Reader` traits — but there's no mechanism for library code to call into the
  host runtime.
- **No trait-based dispatch.** Without `extern fn`, the only way to reach the host is
  through VM builtins, which are invisible to the type system and can't participate in
  trait dispatch.

The goal: build `extern fn` (the bridge to the host) and `std::io` (the Writer pattern),
then remove the print builtins entirely.

---

## 1. `core::platform` — the platform boundary (Layer 1)

The extern "C" boundary is a **platform concern**, not an I/O concern. It belongs in `core`,
not `std::io`. This follows Go (`runtime.write`), Rust (`std::sys`), and Zig (`std.os`) —
all isolate the platform boundary in one place, then build safe APIs on top.

### 1.1 Architecture

```
core/platform.ax        ← extern "C" wrappers around libc (write, read, close, etc.)
  └─ std/io.ax          ← safe Axiom API built on core::platform (print, println)
```

`core::platform` owns the entire unsafe platform boundary. `std::io` imports from it and
wraps with safe, user-facing APIs. Users never touch `core::platform` directly.

### 1.2 Syntax

```axiom
// core/platform.ax — extern "C" fn declarations (no body, dispatched by VM)
pub extern "C" fn write(fd: Int, let buf: Bytes) -> Int;
pub extern "C" fn read(fd: Int, inout buf: Bytes) -> Int;
pub extern "C" fn close(fd: Int) -> Int;
```

- `extern "C" fn` has a signature but no body. Terminated with `;`.
- It's `pub` or private like any other function.
- It lives in `core::platform` — the single platform boundary module.
- **Buffers are values + conventions, not reference types** (`&[U8]` is invalid Axiom —
  §4.1 forbids reference types). A byte buffer is `Bytes`; `let buf` is a read-only borrow
  (`const void*`), `inout buf` an exclusive mutable borrow (`void*`). The libc `len` is
  synthesized from the buffer at the ABI boundary. See
  [`extern-buffers-and-path-unification.md`](extern-buffers-and-path-unification.md).

### 1.3 IR representation

Extern fns use `IrFunction.is_extern: bool` (already implemented). The VM dispatches them
via the builtin table — same mechanism as hardcoded builtins, but triggered by the `is_extern`
flag rather than name matching.

When real FFI arrives (Cranelift JIT), `core::platform` becomes the single place where
`libc::write`, `libc::read`, etc. are declared — not scattered across `std::io`, `std::fs`, etc.

### 1.4 VM host callback mechanism

Extern fns are dispatched via the builtin table (already implemented). The VM looks up the
function name, calls the Rust implementation, stores the result.

### 1.5 Why `core`, not `std::io`

| Reason | Detail |
|---|---|
| **Platform boundary is reusable** | `std::fs`, `std::net`, `std::process` all need `write`/`read`/`close` — declare once in `core` |
| **Matches Go/Rust/Zig** | Go: `runtime.write`, Rust: `std::sys`, Zig: `std.os` — all isolate platform boundary |
| **Testing clarity** | Test `core::platform` primitives in isolation, then test `std::io` wrappers separately |
| **Unsafe boundary is `core`'s job** | `core` = language primitives + unsafe boundaries. `std` = safe user-facing API |
| **Future FFI story** | When real FFI arrives, `core::platform` is where `libc` bindings live — one place |

---

## 2. Writer trait & std::io (Layer 2)

With `core::platform` providing the platform boundary, `std::io` builds safe user-facing
APIs on top. Users import `std::io`, never `core::platform` directly.

### 2.1 The Writer trait — `std::io`

```axiom
// core/io.ax
pub trait Writer {
    fn write(let self, let data: &[U8]) -> Result<usize, IoError>;
    fn flush(let self) -> Result<(), IoError>;

    // Default method — convenience
    fn write_all(let self, let data: &[U8]) -> Result<(), IoError> {
        let written = 0
        while written < data.len() {
            let n = try self.write(data[written..])
            written = written + n
        }
        Ok(())
    }

    fn write_str(let self, let s: &str) -> Result<(), IoError> {
        self.write_all(s.as_bytes())
    }
}
```

### 2.2 The Reader trait — `core::io`

```axiom
// core/io.ax
pub trait Reader {
    fn read(let self, let buf: &mut [U8]) -> Result<usize, IoError>;
}
```

### 2.3 IoError — `core::io`

```axiom
// core/io.ax
pub enum IoError {
    WriteFailed,
    ReadFailed,
    // expand as needed
}
```

### 2.4 Stdout implementation — `std::io`

```axiom
// std/io.ax
use core::io::{Writer, IoError}
use core::platform::write   // platform boundary — extern "C" fn

pub struct Stdout { }

impl Writer for Stdout {
    fn write(let self, let data: &[U8]) -> Result<usize, IoError> {
        let n = write(1, data, data.len())  // fd 1 = stdout
        if n < 0 {
            Err(IoError::WriteFailed)
        } else {
            Ok(n as usize)
        }
    }

    fn flush(let self) -> Result<(), IoError> {
        Ok(())  // stdout is line-buffered by the host
    }
}
```

### 2.5 Convenience functions — `std::io`

```axiom
// std/io.ax

pub fn print(let s: &str) {
    Stdout {}.write_str(s)    // panics on error — acceptable for convenience
}

pub fn println(let s: &str) {
    print(s)
    print("\n")
}
```

### 2.6 Prelude integration

`print` and `println` are added to the prelude (see `modules-design.md` Phase 4) so
they're available without `use`:

```axiom
// prelude.ax (compiler-internal)
use std::io::{print, println}
use core::option::{Option, Some, None}
use core::result::{Result, Ok, Err}
```

`println("hello")` works in any file without imports. The user can still write
`use std::io::println` explicitly for clarity.

### 2.7 What this does NOT include

| Feature | Status | Why |
|---|---|---|
| `string::format` | `[Deferred → alongside Display]` | Needs `Display` trait + format machinery — separate effort |
| `Display` / `Debug` traits | `[Deferred → with traits impl]` | Requires traits to be fully working first |
| `dyn Writer` (trait objects) | `[Deferred → v1.1]` | Requires vtable generation — static dispatch is enough for now |
| `BufWriter`, `BufReader` | `[Deferred → v2]` | Wrapping types — needs trait objects or generics to be ergonomic |
| File I/O (`std::fs`) | `[Deferred → v2]` | Separate extern fn set (`open`, `read`, `write`, `close`) |
| `read_line` | `[Deferred → with stdin]` | Needs `Reader` trait + `Stdin` implementation |
| `dbg` | `[Deferred → with Debug]` | Needs `Debug` trait for value formatting |
| `Stderr` | `[Deferred → with Stdout]` | Same pattern, trivial once Stdout works |

---

## 3. Implementation phases

### Phase 1 — Extern fn syntax & IR

**Goal:** `extern fn` parses, type-checks, and lowers to IR.

- [ ] Parse `extern fn name(params) -> RetType;` (no body) → `Item::ExternFn` in HIR
- [ ] Type check: validate parameter and return types
- [ ] IR: new `IrInstr::ExternCall { dst, name, args }`
- [ ] IR lowering: calls to `extern fn` emit `ExternCall` instead of `Call`
- [ ] Error: extern fn with a body
- [ ] Error: non-extern fn without a body
- [ ] Golden file tests for extern fn IR output

**Test:** Define `extern fn add(a: I32, b: I32) -> I32;`. Lower to IR. IR contains
`ExternCall` instruction.

### Phase 2 — VM extern fn support

**Goal:** The VM can invoke host-provided function implementations.

- [ ] VM: add `extern_fns: HashMap<String, ExternFn>` field
- [ ] VM: add `register_extern` API
- [ ] VM: handle `ExternCall` instruction — look up and invoke host callback
- [ ] VM: store return value in destination register
- [ ] Error: calling an extern fn with no registered callback → clear runtime error
- [ ] VM trace output: `ExternCall` appears in traces

**Test:** Define `extern fn add(a: I32, b: I32) -> I32;` in Axiom. Register a Rust
callback that adds. Call from Axiom code. Result is correct.

### Phase 3 — core::io

**Goal:** `Writer` and `Reader` traits exist in the compiler's `core/` directory.

- [ ] Create `core/io.ax` with `IoError` enum
- [ ] Create `core/io.ax` with `Writer` trait (write, flush, write_all, write_str)
- [ ] Create `core/io.ax` with `Reader` trait (read)
- [ ] Verify: these files compile (parse + type check) as part of the module system

**Test:** `core/io.ax` parses and type-checks without errors.

### Phase 4 — std::io

**Goal:** `Stdout`, `print`, `println` work through the Writer trait.

- [ ] Create `std/io.ax` with `extern fn _write_stdout`
- [ ] Create `std/io.ax` with `Stdout` struct
- [ ] Implement `Writer for Stdout` — delegates to `_write_stdout`
- [ ] Define `print`, `println` as regular functions in `std/io.ax`
- [ ] Register `_write_stdout` as a VM extern callback (Rust `print!` underneath)
- [ ] End-to-end test: `println("hello")` prints to stdout

**Test:** `println("hello")` works. Writing to a custom `VecWriter` in tests works.

### Phase 5 — Prelude & builtin removal

**Goal:** `print`/`println` come from `std::io`, not VM builtins.

- [ ] Update prelude to import `print`, `println` from `std::io`
- [ ] Remove `print`/`println` from VM builtins list
- [ ] Remove `is_builtin("print")` / `is_builtin("println")` checks from VM
- [ ] Update IR golden files — `println` calls go through `std::io::println` →
  `_write_stdout` → `ExternCall`
- [ ] Update VM trace golden files
- [ ] All existing tests still pass (same behavior, different path)

**Test:** `println("hello")` works without any `use` statement. No builtins remain.

---

## 4. Dependency graph

```
modules-design.md (Phase 1-3)          modules-design.md (Phase 4)
        │                                       │
        ▼                                       ▼
Phase 1 (extern fn syntax & IR)        Phase 5 (prelude & builtin removal)
        │                                       ▲
        ▼                                       │
Phase 2 (VM extern fn support)                  │
        │                                       │
        ▼                                       │
Phase 3 (core::io) ──────────► Phase 4 (std::io)┘
```

- Phases 1–2 (extern fn) are independent of modules — could be built in parallel.
- Phases 3–4 (core/std io) require both modules and extern fn.
- Phase 5 (prelude + cleanup) requires modules Phase 4 (prelude) and std::io Phase 4.

---

## 5. Compiler architecture changes

### 5.1 Changes to existing crates

| Crate | Change |
|---|---|
| `axiom-parser` | Parse `extern fn` declarations (no body, `;` terminator) |
| `axiom-hir` | Add `Item::ExternFn { name, params, ret_ty }` node |
| `axiom-typeck` | Type check extern fn signatures; validate calls match signatures |
| `axiom-ir` | `IrInstr::ExternCall`; lower extern fn calls to `ExternCall` |
| `axiom-vm` | `extern_fns` map, `ExternCall` handler, `register_extern` API |
| `axiom-driver` | Register std extern callbacks when creating VM |

### 5.2 VM changes (minimal)

The VM gets two additions:
1. `extern_fns: HashMap<String, ExternFn>` — host callbacks
2. `ExternCall` instruction handler — look up + invoke

Everything else (module resolution, visibility, name resolution) happens before the IR
reaches the VM. The VM sees flat IR with qualified names, same as today.

---

## 6. Testing strategy

### 6.1 Extern fn tests

- Define `extern fn`, register Rust callback, call from Axiom → correct result
- Multiple extern fns with different signatures
- Error: extern fn not registered → runtime error with clear message
- Extern fn as method on a struct

### 6.2 std::io tests

- `println("hello")` prints to stdout (integration test)
- Custom `Writer` implementation: `VecWriter` captures output → testable
- `Writer::write_all` default method works
- `Writer::write_str` delegates correctly
- Prelude: `println` available without `use`

### 6.3 Golden file updates

- IR golden files: `println` no longer a `Call` to a builtin, but a normal call chain
  through `std::io::println` → `_write_stdout` → `ExternCall`
- VM trace golden files: updated call chain

---

## 7. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Extern fn name collisions | Two modules register same name | **Resolved (ef553e1).** FnDef carries `module_path` set during multi-file resolution. IR lowering qualifies names (e.g., `core::platform::write`). VM and invariant checker handle both bare and qualified forms. |
| Missing extern callback at runtime | Panic or UB | Clear error message: "extern function 'io._write_stdout' not registered" |
| Writer trait needs generics too early | Scope creep | `Writer` works without generics — each impl is for a concrete type. Generics come later. |
| Prelude importing `print` before `std::io` exists | Ordering dependency | Phase 5 is explicitly last — prelude update happens after std::io is working |
| `core` ↔ `std` circular deps | Compiler stdlib broken | Strict layering: `core` never imports from `std`. `Writer` lives in `core`, `Stdout` in `std`. |
