# axiom-vm

Register-IR interpreter for the Axiom language.

Takes an `Ir` (from `axiom-ir`) and executes it by walking basic blocks,
dispatching instructions against a register file, and managing a call stack.

## Scope (v0)

- Naive copy semantics (values cloned on Copy/pass)
- Simple Vec-backed heap arena with manual alloc/free
- Synchronous execution, no concurrency
- Builtins: `print`, `println`
- Struct creation and field access
- Enum creation (Call-based constructors) and pattern matching
- Match with payload binding (TupleStruct patterns)
- Control flow: if/else, match, loops (break/continue)
- Function calls and returns

## Testing

- Unit tests per instruction/terminator variant
- Golden trace tests (`.trace` files in `tests/fixtures/`)
- Exhaustiveness invariants (divergence guards)
- FFI consistency and parity test stubs

## Run tests

```bash
cargo test -p axiom-vm
```
