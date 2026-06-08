# axiom-vm

Register-IR interpreter for the Axiom language.

Takes an `Ir` (from the `ir` crate) and executes it by walking basic blocks,
dispatching instructions against a register file, and managing a call stack.

## Scope (v0)

- Naive copy semantics (values cloned on Copy/pass)
- Simple Vec-backed heap arena with manual alloc/free
- Synchronous execution, no concurrency
- Builtins: only the irreducible floor — `format`, `String::as_bytes`, `Bytes::len`,
  per-scalar `hash_raw` (`print`/`println`/`List`/`Map` are now real stdlib `.ax` code)
- Struct creation and field access
- Enum creation (Call-based constructors) and pattern matching
- Match with payload binding (TupleStruct patterns)
- Control flow: if/else, match, loops (`break`/`continue` lower to block jumps)
- Function calls and returns; method calls on type-param receivers dispatch by runtime type
- Hard step cap (`AXIOM_VM_MAX_STEPS`, default 50M) — a runaway loop errors with
  `StepLimitExceeded` instead of spinning/OOMing (tracing keeps an unbounded per-instr log)

## Files

| File | Responsibility | Key items |
|---|---|---|
| `src/lib.rs` | Crate root; public API | `Vm`, `Vm::new`, `Vm::run` |
| `src/value.rs` | Runtime value representation | `Value` enum (Int, Bool, Float, Unit, Str, Struct, Enum, Bytes, Place), `Display` |
| `src/frame.rs` | Call frame: register file, locals, instruction pointer | `Frame`, `Frame::new` |
| `src/error.rs` | VM error types (`thiserror`) | `VmError` (StepLimitExceeded, StackOverflow, etc.) |
| `src/trace.rs` | Per-instruction execution trace log | `TraceEntry`, `TraceLog` |
| `src/exec/mod.rs` | Execution dispatch hub | Re-exports `instr`, `terminator`, `binop`, `builtins` |
| `src/exec/instr.rs` | Instruction dispatch: exhaustive `match` over `IrInstr` | `exec_instr` |
| `src/exec/terminator.rs` | Terminator dispatch: `Return`, `Jump`, `Branch`, `Halt`, `Panic` | `exec_terminator` |
| `src/exec/binop.rs` | Binary operator evaluation | `eval_binop` |
| `src/exec/builtins.rs` | Builtin intrinsics (`format`, `hash_raw`, `String::as_bytes`, etc.) | `call_builtin` |
| `tests/golden.rs` | IR golden trace tests (`.trace` check-in) | — |
| `tests/invariants.rs` | Exhaustiveness coverage invariants | — |
| `tests/*_e2e.rs` | End-to-end tests exercising real stdlib code on the VM | List, Map, traits, generics, subscripts, format, etc. |

## Testing

- Unit tests per instruction/terminator variant
- Golden trace tests (`.trace` files in `tests/fixtures/`)
- End-to-end collection tests — exercise the real stdlib `List`/`Map` on `HeapBuffer<T>`,
  trait dispatch, generic structs/enums, subscripts, and more
- Exhaustiveness invariants (divergence guards)

## Run tests

```bash
cargo test -p vm
```
