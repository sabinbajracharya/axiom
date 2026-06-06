# Register-IR VM Design (`axiom-vm`)

> **Status:** Design approved, implementation starting.
> **Scope:** v0 — naive copy semantics, no ownership pass, no Perceus.

## Overview

A synchronous, stack-framed, register-IR interpreter. Takes an `IrModule` (from
`axiom-ir`) and executes it by walking basic blocks, dispatching instructions
against a register file, and managing a call stack.

Pairs with the future Cranelift codegen as the "shared FFI layer" backend
(DESIGN_SPEC §13.2). Both consume one IR; both delegate runtime semantics to
shared builtins so they cannot diverge.

## Crate structure

```
crates/axiom-vm/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs              # Vm::new(ir), vm.run() -> Result<Value>
│   ├── error.rs            # VmError enum (thiserror)
|   |                         #   - UnsupportedIndexBase: indexed op on non-HeapPtr
│   ├── value.rs            # runtime Value enum
│   ├── frame.rs            # StackFrame: register file + current block + IP
│   ├── exec/
│   │   ├── mod.rs          # run_frame() dispatch loop
│   │   ├── instr.rs        # exec_instr(): exhaustive match on IrInstr
│   │   ├── terminator.rs   # exec_terminator(): exhaustive match on Terminator
│   │   ├── binop.rs        # exec_binop(): all 18 BinOp variants
│   │   └── builtins.rs     # print/println — the only FFI surface for now
│   └── trace.rs            # ExecutionTrace: records every instr for debugging
|   |                         #   - trace.output(): real program output only
└── tests/
    ├── golden.rs                  # trace golden tests (.trace files)
    ├── invariants.rs              # exhaustiveness divergence guards
    ├── mutable_subscript_e2e.rs   # end-to-end: indexed-place write on List + user struct
    ├── output_assertion_guard.rs  # H1 drift guard: bans trace.format() in *e2e.rs suites
    ├── place_assign_e2e.rs        # end-to-end: assignment to struct fields + heap buffers
    ├── place_assign_matrix.rs     # H3 coverage matrix: {target}×{op}×{base}
    ├── list_e2e.rs                # end-to-end: List operations
    ├── map_e2e.rs                 # end-to-end: Map operations
    ├── subscript_e2e.rs           # end-to-end: subscript reads
    ├── parity.rs                  # (stub) future JIT-vs-interp parity test
    ├── ffi_consistency.rs         # (stub) future FFI surface consistency
    └── fixtures/
        ├── hello.ax + hello.trace
        ├── arithmetic.ax + arithmetic.trace
        ├── control_flow.ax + control_flow.trace
        ├── functions.ax + functions.trace
        ├── loops.ax + loops.trace
        ├── match_expr.ax + match_expr.trace
        └── multi_fn.ax + multi_fn.trace
```

## Core types

### Value — runtime representation

```rust
enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Unit,
    Struct { type_name: String, fields: Vec<(String, Value)> },
    Enum { type_name: String, variant: String, payload: Vec<Value> },
    List(Vec<Value>),
    HeapPtr(usize),  // index into HeapArena
}
```

Clone-heavy for v0. No Rc, no RefCell. Arena indices for heap.

### HeapArena — simple heap

```rust
struct HeapArena {
    slots: Vec<Option<HeapSlot>>,
}

struct HeapSlot {
    data: Vec<Value>,
    refcount: u32,
}
```

Alloc returns an index. Free sets `None`. Get/Set index into `data`.
Refcount tracked but not enforced (v0 = naive).

### StackFrame — per-call register file

```rust
struct StackFrame {
    fn_name: String,
    regs: Vec<Value>,                  // indexed by Reg.0
    block_index: usize,                // current block in blocks[]
    instr_index: usize,                // current instruction within block
    blocks: Vec<IrBlock>,              // the function's blocks
    label_map: HashMap<String, usize>, // block label → block index
    loop_stack: Vec<LoopFrame>,        // for Break/Continue
}
```

### Vm — top-level executor

```rust
pub struct Vm {
    ir: Ir,
    fn_map: HashMap<String, usize>, // function name → index
    heap: HeapArena,
    call_stack: Vec<StackFrame>,
    trace: Option<ExecutionTrace>,   // None = no tracing
}
```

Public API:
- `Vm::new(ir: Ir) -> Self`
- `vm.set_tracing(enabled: bool)` — toggle trace recording
- `vm.run() -> Result<Value>` — execute entry function
- `vm.run_function(name: &str, args: Vec<Value>) -> Result<Value>` — call specific function
- `vm.take_trace() -> ExecutionTrace` — extract recorded trace
- `vm.trace_output() -> String` — concatenated real program output (filtered
  to `output` entries only — the basis for behavioural e2e assertions, as
  distinct from full-trace text goldens; see
  [`docs/mutable-subscript-design.md`](docs/mutable-subscript-design.md) §7 H1)

## Execution model

1. **Bootstrap:** resolve entry function, create initial `StackFrame` with
   params bound to regs.
2. **Main loop** (`run_frame`):
   - Fetch `instrs[instr_index]` from current block.
   - Dispatch via exhaustive `match` on `IrInstr` (no wildcard — divergence
     guard #1).
   - Write result to `regs[dst.0]`.
   - Increment `instr_index`.
   - When `instr_index >= instrs.len()`, execute `terminator`.
3. **Terminator dispatch** (exhaustive match):
   - `Return(val)` → pop frame, write val to caller's dst reg.
   - `Jump(label)` → set `block_index`, reset `instr_index = 0`.
   - `Branch(cond, t, f)` → read cond, jump to t or f.
   - `Match(scrutinee, arms, fallback)` → pattern match, jump.
   - `Break(val)` → unwind loop_stack, jump to after-loop block.
   - `Continue` → jump to loop head.
   - `Unreachable` → runtime error.
4. **Function calls:** push new `StackFrame`, bind args to param regs, run
   until `Return`.
5. **Builtins** (`print`, `println`): intercept by name before frame push,
   write to output buffer.

## Testing strategy (5 layers)

### Layer 1: Unit tests (inline `#[cfg(test)]`)

Every exec module gets inline tests:
- `test_exec_const_int`, `test_exec_binop_add`, `test_exec_branch_true`, etc.
- Construct `IrModule` by hand, run through `Vm`, assert result.
- Many small focused tests, one per instruction variant.

### Layer 2: Golden trace tests (`tests/golden.rs`)

`.ax` source → full pipeline → IR → VM with tracing → `.trace` golden file.

Trace format (one line per step):
```
[fn main] %1 = Const(42)
[fn main] %2 = Const(3)
[fn main] %3 = BinOp Add(%1, %2) => 45
[fn main] Return %4
```

- `UPDATE_SNAPSHOTS=1` to regenerate.
- Same `check_golden` helper pattern as IR golden tests.
- 7 fixtures matching the existing IR fixtures.

### Layer 3: Exhaustiveness invariants (`tests/invariants.rs`)

Three divergence guards (from DESIGN_SPEC §13.2):
1. **IrInstr coverage:** every variant has an execution arm (no wildcard).
2. **Terminator coverage:** every variant has an execution arm.
3. **BinOp coverage:** every variant has an evaluation arm.
4. **Indexed-base guard:** `Index`/`IndexSet` on a non-`HeapPtr` base returns
   `UnsupportedIndexBase` — never silently falls through to no-op/`Unit`
   ([`docs/mutable-subscript-design.md`](docs/mutable-subscript-design.md) §7 H4).

### Layer 4: FFI consistency (`tests/ffi_consistency.rs`)

Stub for now (no Cranelift yet). Will later verify builtin functions registered
in VM match those declared for codegen. Divergence guard #2.

### Layer 5: Parity test (`tests/parity.rs`)

Stub for now. Will later run a corpus through both VM and Cranelift, diff
outputs. Divergence guard #3.

## Implementation order

| Step | What | Tests |
|------|------|-------|
| 1 | Scaffold crate + Value + Error | Value unit tests |
| 2 | Frame + register file | Frame construction tests |
| 3 | Instruction dispatch (14 variants + binops) | Unit tests per instr |
| 4 | Terminator dispatch (7 variants + pattern match) | Unit tests per terminator |
| 5 | Call stack + builtins | Call/return, recursive tests |
| 6 | Heap arena (alloc/free/get/set) | Heap unit tests |
| 7 | Vm::run() + golden traces | 7 trace golden fixtures |
| 8 | Invariants + README + stubs | Exhaustiveness checks |

## Constraints (from RUST_CONVENTIONS.md)

- Zero `unsafe` (not a codegen crate).
- No `RefCell`, `Cell`, `Mutex`, `RwLock` (banned types).
- No `Rc<RefCell<T>>` — arena indices for heap.
- No `async` — synchronous execution.
- One `thiserror` enum (`VmError`).
- No `unwrap`/`panic!` on user-reachable paths.
- 600-line file cap, 60-line function cap.
- Exhaustive `match` — no wildcard arms on IrInstr/Terminator/BinOp.
- Test naming: `test_<what>_<scenario>`.
- Per-folder `README.md`.

## Verification

After each step:
```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test -p axiom-vm
```

Final gate (all crates):
```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
```
