# axiom-vm

Register-IR interpreter for the Axiom language.

Takes an `Ir` (from `axiom-ir`) and executes it by walking basic blocks,
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

## Testing

- Unit tests per instruction/terminator variant
- Golden trace tests (`.trace` files in `tests/fixtures/`)
- End-to-end collection tests (`tests/list_e2e.rs`, `tests/map_e2e.rs`) — exercise the real
  stdlib `List`/`Map` on `HeapBuffer<T>`
- Exhaustiveness invariants (divergence guards)
- FFI consistency and parity test stubs

## Run tests

```bash
cargo test -p axiom-vm
```
