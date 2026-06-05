# Extern Buffers & Path Unification

> **Status:** Planned. Fixes the builtin/extern architecture debt identified in the
> June 2026 review. The two-layer design (`core::platform` extern boundary → `std::io`
> safe wrappers, dispatch via `IrFunction.is_extern`) is **correct and kept**. What this
> plan repairs is *unfinished-migration debt* plus one unresolved type question.
>
> **Companion docs:** [`io-design.md`](io-design.md) (the two-layer architecture this
> completes), [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §4 (memory model — the source of the
> "no reference types" rule), [`ir-design.md`](ir-design.md) (extern call / ABI lowering).

---

## 1. Root cause

Three of the four reported issues share one root cause: **there are two compiler paths
that load *different* stdlib sets.**

- **Single-file path** (`axiom-typeck::stdlib::with_stdlib`) prepends `list.ax + map.ax +
  io.ax` — but **not** `platform.ax`. So `write` inside `io.ax` resolves to nothing and
  falls through to the VM's name-matched `"write"` builtin.
- **Multi-file path** loads `platform.ax` as a real module. Its `&[U8]` parameter syntax
  hits `lower_ty`'s catch-all → `NotYetSupported` → `HirTy::Error`, which **silently
  suppresses** the arity/type mismatch between the 3-param libc declaration and the
  2-arg `write(1, s.as_bytes())` call.

This divergence violates Axiom's own "one obvious way" principle at the compiler level and
is what lets the `&[U8]` bug and the premature "✅ Done" rows hide. Unifying the paths
dissolves most of the debt.

## 2. The buffer-type question (the only deep one)

The design spec (§4.1) forbids reference *types*: "you cannot declare, store, or return a
reference to T." But `platform.ax` was written with Rust's `&[U8]` / `&mut [U8]` as a
placeholder. The spec already answers how to express "C receives a pointer" **without** a
reference type:

> The conventions *are* the calling convention (§4.2: `let` ≈ `&T`, `inout` ≈ `&mut T`).

So a buffer parameter is a **byte-buffer value passed with a convention**:

| Rust placeholder | Axiom (spec-faithful) | C ABI meaning |
|---|---|---|
| `buf: &[U8]` | `let buf: Bytes` | `const void* + len` (read-only, borrowed for the call) |
| `buf: &mut [U8]` | `inout buf: Bytes` | `void* + len` (writable, exclusively borrowed for the call) |

> **Note — `U8` is not an Axiom type.** The spec (§3) rejects the `i8/u8/...` integer zoo;
> the byte scalar is `Byte`, and the byte buffer is `Bytes` (what `String::as_bytes()`
> already returns and the VM represents as `Value::Bytes`). So the byte buffer is `Bytes`,
> **not** `[U8]`. The convention — not a reference type — supplies the `const`/mutable
> distinction (§4.2: `let` ≈ `&T`, `inout` ≈ `&mut T`). No stored alias, no escaping
> pointer — fully MVS-faithful.

**No `RawPtr<T>` is added.** A raw pointer is reference-like (it *stores* an alias), which
§4.1 forbids. Defer it until something genuinely needs to *hold* a pointer across calls
(mmap regions, FFI struct fields). libc `read`/`write`/`close` never do.

We still add a general **`HirTy::Slice(Box<HirTy>)`** (surface syntax `[T]`) — needed for
`[1, 2, 3]` array literals and a future `[Byte]` spelling — lowering onto the typeck layer's
existing `Ty::HeapBuffer(Box<Ty>)`. The byte buffer specifically uses `Bytes` today;
unifying `Bytes` with `[Byte]` waits on wiring the `Byte` scalar primitive into the type
checker (deferred — no current need).

### Arity reconciliation

The Axiom-level signature becomes `fn write(fd: Int, let buf: Bytes) -> Int` — **two
params**, matching the existing `write(1, s.as_bytes())` call. libc's `len` is synthesized
by ABI lowering from the buffer's length. `read` becomes
`fn read(fd: Int, inout buf: Bytes) -> Int` — the `inout` buffer is writable, the return
value is bytes-read.

### Extern fns have no body

A second gap surfaced: the type checker ran its return-type-vs-body check on `extern` fns,
whose (empty) body is `Unit` — so `-> Int` wrongly reported a mismatch. Fixed: when
`extern_abi.is_some()`, record the signature and skip body reconciliation (the platform
supplies the body).

## 3. Work plan (each step is one commit; TDD; gate must pass)

1. ✅ **docs** — this file.
2. ✅ **`HirTy::Slice` + parser `[T]` slice type.** `SliceType` SyntaxKind + grammar
   (`[` ty `]`); AST `SliceType` view with `element_type()`; `is_type_kind`;
   `HirTy::Slice(Box<HirTy>)`; lowered in `lower/ty.rs`; serialized as `[T]`. Tested.
3. ✅ **typeck lowering** — `HirTy::Slice(inner)` → `Ty::HeapBuffer(inner)`. Tested.
4. ✅ **Rewrite `platform.ax`** — `let buf: Bytes` / `inout buf: Bytes`, 2-param `write`,
   `inout` `read`. Type checker skips body/return reconciliation for bodiless `extern` fns.
   Buffer-type decision recorded in `DESIGN_SPEC.md` §11.1. Multi-file HIR goldens regenerated.
5. ✅ **Extern dispatch** — `vm/exec/builtins.rs` resolves platform externs off `is_extern`
   through the closed `PlatformFn` enum (`resolve_extern`/`call_extern`), not a `matches!`
   name list. (Implemented as an exhaustive enum rather than a `register_extern` table —
   simpler, no `dyn`, and a no-wildcard drift guard; signature correctness stays the type
   checker's job.) New `VmError::ExternNotImplemented`.
6. ✅ **Cleanup** — removed all `print`/`println`/`write` *VM* builtins. The `print`/`println`
   stand-in in the type checker's `builtin_fn` was retired in a follow-up
   ([`string-format-and-print-retire.md`](string-format-and-print-retire.md)): their real
   `String`-only signatures are now seeded from `stdlib/io.ax`, and `string::format` (the one
   variadic formatting intrinsic) bridges non-string values. Only `todo` remains in
   `builtin_fn`.
7. ✅ **Unify the paths** — `with_stdlib` now prepends `core/platform.ax` (before `io.ax`), so
   `write`/`read`/`close` resolve to real externs in single-file mode too; the VM trace harness
   prepends the stdlib via the now-public `typeck::with_stdlib`.

### Fixes discovered en route (not in the original plan)

- **Trailing-semicolon discard.** Unifying the paths surfaced a latent block-lowering bug: the
  final `ExprStmt` was always promoted to the block tail, ignoring a trailing `;`. Per
  DESIGN_SPEC §16 a trailing `;` discards the value (block → `()`). Added
  `ExprStmt::has_semicolon()` and fixed `lower_block`; this is what lets `io.ax`'s
  `print`/`println` discard the `Int` from `write`.
- **Primitive-receiver method qualification.** `s.as_bytes()` on a `String` was lowering to a
  bare `as_bytes` MethodCall (dead until `println` became real), which the VM couldn't find.
  `type_name_from_ty` now qualifies `String`/`Int`/`Float`/`Bool` so `String::as_bytes`
  dispatches.

## 4a. Status: complete

All steps landed; `fmt` + `clippy -D warnings` + the full test suite pass (51 suites). The
remaining deferred items (real FFI, VM `read`, the prelude that retires the `builtin_fn`
stand-in, unifying `Bytes` with `[Byte]`) are tracked above and in `io-design.md`.

## 4. Out of scope (deferred, with reason)

| Item | Why deferred |
|---|---|
| `RawPtr<T>` | Reference-like; no current need (§2). Revisit for mmap/FFI structs. |
| Real FFI (`dlsym`/`libloading`) | Needs Cranelift JIT (io-design.md). VM keeps a callback table. |
| Fixed-size arrays `[T; N]` | Only the dynamically-sized slice `[T]` is needed now. |
| ABI `(ptr, len)` emission | Only matters at the Cranelift backend; the VM passes a `Bytes` value. |
