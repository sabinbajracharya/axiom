# axiom-vm

Register-IR interpreter for the Axiom language.

Takes an `IrModule` (from `axiom-ir`) and executes it by walking basic blocks,
dispatching instructions against a register file, and managing a call stack.

## Scope (v0)

- Naive copy semantics (values cloned on Copy/pass)
- Simple Vec-backed heap arena with manual alloc/free
- Synchronous execution, no concurrency
- Builtins: `print`, `println`

## Testing

- Unit tests per instruction/terminator variant
- Golden trace tests (`.trace` files in `tests/fixtures/`)
- Exhaustiveness invariants (divergence guards)
- FFI consistency and parity test stubs

## Run tests

```bash
cargo test -p axiom-vm
```
