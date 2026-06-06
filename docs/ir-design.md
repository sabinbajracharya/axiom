# IR Design — Register-Based, CFG, Monomorphized

> **Status:** authoritative for the IR layer. Binding before code is written.
> **Decisions baked in:** register-based IR with explicit basic blocks + terminators
> (Oxy-shaped), ownership-annotated operands (deferred to v1), explicit RC ops (deferred to v1),
> monomorphized generics as separate IR functions, dual backend (Cranelift + IR interpreter).
> **Prerequisites:** type checker (THIR), monomorphizer (`MonoResult`).
> **Companion docs:** [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §13,
> [`generics-design.md`](generics-design.md) §7 (the generics consumer),
> [`traits-design.md`](traits-design.md) (trait dispatch in IR),
> [`typeck-testing.md`](typeck-testing.md) (the layer below),
> [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

---

## 0. The concern this answers

The compiler pipeline currently ends at THIR (typed HIR). The monomorphizer produces
`MonoResult` with concrete function signatures but has no consumer. There is no
intermediate representation that bridges type checking → codegen.

Without an IR, there's no way to:
- Lower HIR expressions to a form suitable for codegen
- Represent control flow explicitly (basic blocks, terminators)
- Emit monomorphized generic function instances
- Insert RC operations (incref/decref/reuse) in v1
- Target Cranelift or a WASM interpreter

The fear: **we build codegen directly from HIR/THIR**, coupling the backend to the
frontend's tree structure, making optimizations impossible and RC insertion intractable.

---

## 1. The design, stated plainly

### 1.1 What the IR is

A **register-based intermediate representation** with explicit control flow graphs.
Each function is a sequence of basic blocks. Each block contains a list of instructions
that produce register values, ending with a single terminator that transfers control
to another block.

```
fn add(let a: Int, let b: Int) -> Int {
  entry:
    %0 = Const Int(42)
    %1 = BinOp + %a %b
    Return %1
}
```

### 1.2 Design principles

- **Register-based.** Every intermediate value lives in a virtual register (`%0`, `%1`, ...).
  Registers are SSA-like within a block (assigned once). No stack, no explicit memory model
  in v0.
- **CFG structure.** Control flow is explicit: basic blocks + terminators. No nested
  expressions — `if`/`match`/`loop` become branches between blocks.
- **Types preserved.** Every register carries a `Ty` (from the type checker). The IR
  does not re-infer types — it consumes the THIR's `TypeMap`.
- **Ownership annotations (v1).** Each IR value will know whether it is owned or borrowed.
  Deferred to the ownership pass.
- **Explicit RC ops (v1).** `incref`/`decref`/`reuse` instructions inserted by the RC
  pass. Deferred — v0 has naive memory (no RC).
- **Monomorphized.** Generic functions appear as concrete instances with mangled names
  (`max__Int`). Type parameters are gone — everything is fully concrete.
- **Dual backend.** One IR feeds both Cranelift (native) and the register-IR interpreter
  (WASM). No IR-level divergence between backends.

### 1.3 What the IR is NOT

- **Not SSA.** Registers are assigned once per block, but phi nodes are not used.
  Block parameters (if needed) or copies handle cross-block data flow.
- **Not optimized.** v0 IR is a direct lowering from HIR. No dead code elimination,
  constant folding, or inlining. Optimization is a future concern.
- **Not a memory model.** v0 IR has no heap/stack distinction, no allocation instructions.
  The memory model (RC, reuse) is layered on in v1.

---

## 2. The IR type universe

### 2.1 The program

```rust
/// The complete IR program.
#[derive(Debug, Clone)]
pub struct Ir {
    pub functions: Vec<IrFunction>,
    pub entry: usize,  // index of the entry function (main)
}
```

### 2.2 Functions

```rust
#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    /// Type parameters (empty after monomorphization).
    pub type_params: Vec<TypeParamId>,
    /// Links back to the generic definition, if this is a monomorphized instance.
    pub generic_origin: Option<GenericOrigin>,
    pub params: Vec<IrParam>,
    pub return_type: Ty,
    pub blocks: Vec<IrBlock>,
    pub next_reg: u32,
}

#[derive(Debug, Clone)]
pub struct IrParam {
    pub reg: Reg,
    pub name: String,
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub struct GenericOrigin {
    pub generic_name: String,
    pub concrete_args: Vec<Ty>,
}
```

### 2.3 Registers

```rust
/// A virtual register. Assigned once per block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reg(pub u32);
```

### 2.4 Basic blocks

```rust
#[derive(Debug, Clone)]
pub struct IrBlock {
    pub label: String,
    pub instrs: Vec<IrInstr>,
    pub terminator: Terminator,
}
```

### 2.5 Instructions

```rust
#[derive(Debug, Clone)]
pub enum IrInstr {
    /// r = literal constant
    Const { dst: Reg, value: IrConst },
    /// r = op lhs rhs
    BinOp { dst: Reg, op: BinOp, lhs: Reg, rhs: Reg },
    /// r = op src
    UnaryOp { dst: Reg, op: UnaryOp, src: Reg },
    /// r = function(args...)
    Call { dst: Reg, function: String, args: Vec<Reg> },
    /// r = receiver.method(args...)
    MethodCall { dst: Reg, receiver: Reg, method: String, args: Vec<Reg> },
    /// r = base.field
    Field { dst: Reg, base: Reg, field: String },
    /// r = base[index]
    Index { dst: Reg, base: Reg, index: Reg },
    /// r = src (register copy)
    Copy { dst: Reg, src: Reg },
    /// r = Type { field1: v1, field2: v2, ... }
    StructNew { dst: Reg, type_name: String, fields: Vec<(String, Reg)> },
    /// r = Variant(payload...)
    EnumNew {
        dst: Reg,
        type_name: String,
        variant: String,
        payload: Vec<Reg>,
    },
    /// r = heap_alloc(count) — allocate buffer for `count` elements, return pointer.
    HeapAlloc { dst: Reg, count: Reg },
    /// heap_free(ptr) — free a heap-allocated buffer.
    HeapFree { ptr: Reg },
    /// r = heap_get(ptr, index) — read element at index from buffer.
    HeapGet { dst: Reg, ptr: Reg, index: Reg },
    /// heap_set(ptr, index, value) — write value at index in buffer.
    HeapSet { ptr: Reg, index: Reg, value: Reg },
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrConst {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Unit,
}
```

### 2.6 Terminators

```rust
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Return from function (with optional value).
    Return(Option<Reg>),
    /// Unconditional jump to target block.
    Jump { target: String },
    /// Conditional branch: if cond then true_target else false_target.
    Branch {
        cond: Reg,
        true_target: String,
        false_target: String,
    },
    /// Pattern match on scrutinee.
    Match {
        scrutinee: Reg,
        arms: Vec<MatchArm>,
        fallback: String,
    },
    /// Break out of a loop (with optional value).
    Break { value: Option<Reg> },
    /// Continue to next loop iteration.
    Continue,
    /// Unreachable (after diverging expressions like return/break).
    Unreachable,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: IrPattern,
    pub target: String,
}

#[derive(Debug, Clone)]
pub enum IrPattern {
    Wildcard,
    Literal(IrConst),
    Variant {
        type_name: String,
        variant: String,
        bindings: Vec<Reg>,
    },
}
```

---

## 3. HIR → IR lowering

### 3.1 Entry point

```rust
/// Lower a typed HIR program to IR.
pub fn lower(thir: &Thir) -> Ir
```

The lowerer walks `thir.hir.items`, lowers each `FnDef` to an `IrFunction`, and
collects them into an `Ir`. The THIR's `TypeMap` provides types for every `HirId`.

### 3.2 Function lowering

Each `FnDef` becomes an `IrFunction`:
1. Create the entry block
2. Allocate a `Reg` for each parameter
3. Lower the body block → emits instructions into the entry block
4. Add implicit `Return` if the block doesn't end with a terminator

### 3.3 Expression lowering

Each HIR expression lowers to a `Reg` (the register holding its result) and emits
zero or more `IrInstr`s into the current block.

| HIR Expression | IR Instructions | Result Reg |
|---|---|---|
| `Lit(Int(42))` | `Const { dst: %n, value: Int(42) }` | `%n` |
| `Bin(+, a, b)` | lower a → `%a`, lower b → `%b`, `BinOp { dst: %n, +, %a, %b }` | `%n` |
| `Unary(-, x)` | lower x → `%x`, `UnaryOp { dst: %n, -, %x }` | `%n` |
| `Call(f, args)` | lower each arg → `%a_i`, `Call { dst: %n, f, [%a_0, ...] }` | `%n` |
| `MethodCall(recv, m, args)` | lower recv → `%r`, lower args → `%a_i`, `MethodCall { dst: %n, %r, m, [%a_0, ...] }` | `%n` |
| `Field(base, f)` | lower base → `%b`, `Field { dst: %n, %b, f }` | `%n` |
| `Index(base, idx)` | lower base → `%b`, lower idx → `%i`, `Index { dst: %n, %b, %i }` | `%n` |
| `StructLit(T, fields)` | lower each field value → `%f_i`, `StructNew { dst: %n, T, [(name, %f_i), ...] }` | `%n` |
| `ListLit(elems)` | lower each elem → `%e_i`, then desugar to `Call List::new() → %n` + a `MethodCall %n.List::push(%e_i)` per element (no list intrinsic — `List<T>` is stdlib over `HeapBuffer<T>`) | `%n` |
| `HeapAlloc(count)` | lower count → `%c`, `HeapAlloc { dst: %n, %c }` | `%n` (pointer) |
| `HeapFree(ptr)` | lower ptr → `%p`, `HeapFree { ptr: %p }` | — |
| `HeapGet(ptr, idx)` | lower ptr → `%p`, lower idx → `%i`, `HeapGet { dst: %n, %p, %i }` | `%n` |
| `HeapSet(ptr, idx, val)` | lower ptr/idx/val → `%p,%i,%v`, `HeapSet { %p, %i, %v }` | — |
| `If(cond, then, else)` | see §3.4 | merge block's value |
| `Match(scrut, arms)` | see §3.5 | merge block's value |
| `Loop(kind)` | see §3.6 | loop's break value |
| `Block { stmts, tail }` | lower stmts sequentially, lower tail (or Unit) | tail's reg |
| `Assign(target, val)` | lower val → `%v`, `Copy` to target's reg (for `Name`/`Field` targets) | Unit |
| `Assign(Index(base, idx), val)` | ...read back via the same type's read subscript dispatch (`lower_index_read`) before the `BinOp`. | Unit |

### 3.3.1 Index lowering helpers

Indexed reads and writes on library types share a common dispatch pattern. Two
public helpers in `crates/axiom-ir/src/lower/expr.rs` encapsulate it:

- **`lower_index_read(base: Reg, base_ty: &Ty, index: Reg, ctx: &mut FnLowerCtx) -> Reg`** —
  emits the IR for `base[index]` as a read expression. For a `[T]` heap buffer it
  emits the primitive `Index` instruction. For any other type (e.g. `List<T>`) it
  emits a `MethodCall Type::subscript(inout self, index)` using the name-keyed
  dispatch the VM already supports. Called by `lower_expr` for `Expr::Index` and
  by compound-assignment lowering (`lower_assign_index`) for read-back of the old
  element.

- **`lower_index_write(base: Reg, base_ty: &Ty, index: Reg, value: Reg, ctx: &mut FnLowerCtx)`** —
  emits the IR for `base[index] = value`. For a `[T]` heap buffer it emits the
  primitive `IndexSet`. For any other type it emits a `MethodCall
  Type::subscript_set(inout self, index, value)`, dispatching to the **write
  subscript** setter (distinguished from the read subscript by
  `SubscriptDef.is_setter` in HIR).

This split avoids a new IR instruction (`SubscriptSet`): the write half reuses
the existing `MethodCall` infrastructure. The `inout` receiver convention ensures
the callee's mutations are written back to the caller, exactly as `push` already
does.

### 3.4 If lowering

```
lower cond → %cond
Branch { cond: %cond, true_target: "then_0", false_target: "else_0" }

then_0:
  lower then_body → %then_val
  Jump { target: "merge_0" }

else_0:
  lower else_body → %else_val   // or Unit if no else
  Jump { target: "merge_0" }

merge_0:
  // %result is %then_val or %else_val (via copies at end of each branch)
```

### 3.5 Match lowering

```
lower scrutinee → %scrut
Match { scrutinee: %scrut, arms: [...], fallback: "match_fallback_0" }

arm_0:
  lower arm body → %arm0_val
  Jump { target: "match_merge_0" }

arm_1:
  ...

match_merge_0:
  // %result from whichever arm executed
```

### 3.6 Loop lowering

```
Jump { target: "loop_head_0" }

loop_head_0:
  // for conditional loop: lower condition → %cond, Branch to body or exit
  // for infinite loop: unconditional Jump to body
  Jump { target: "loop_body_0" }

loop_body_0:
  lower body
  Jump { target: "loop_head_0" }  // back-edge

loop_exit_0:
  // break jumps here; continue jumps to loop_head_0
```

### 3.7 Name resolution in IR

The lowerer maintains a `HashMap<HirId, Reg>` mapping each binding's `HirId` to the
register that holds its value. When lowering a `PathExpr` that resolves to a `DefId`,
look up the `HirId` of the definition and find its register.

### 3.8 Calling conventions

| Convention | IR representation |
|---|---|
| `let` (borrow) | Pass register directly (read-only in v0; borrow annotation in v1) |
| `inout` (mutable borrow) | Pass register directly (mutability tracked in v1) |
| `sink` (consume) | Pass register (ownership transferred in v1) |

In v0, all conventions are pass-by-register. The ownership/RC layer (v1) will add
incref/decref around calls based on convention.

---

## 4. Monomorphized instances

### 4.1 From MonoResult to IrFunction

The monomorphizer produces `MonoInstance` entries. Each becomes an `IrFunction`:

```rust
// MonoInstance { name: "max__Int", original_name: "max", original_id: HirId(5),
//                param_types: [Int, Int], return_type: Int, type_args: [Int] }
// →
IrFunction {
    name: "max__Int",
    type_params: [],  // empty — fully concrete
    generic_origin: Some(GenericOrigin {
        generic_name: "max",
        concrete_args: vec![Ty::Int],
    }),
    params: [IrParam { reg: Reg(0), name: "a", ty: Ty::Int },
             IrParam { reg: Reg(1), name: "b", ty: Ty::Int }],
    return_type: Ty::Int,
    blocks: [...],  // cloned from the generic FnDef's body, with types substituted
    next_reg: ...,
}
```

### 4.2 Body cloning

To produce the body of a monomorphized instance:
1. Look up the original `FnDef` by `original_id` in the HIR
2. Clone its body HIR
3. Apply the type substitution (`TypeParamId → concrete Ty`) to all type annotations
4. Lower the cloned body to IR blocks

This happens during IR lowering, not in the monomorphizer (which stays metadata-only).

### 4.3 Call site resolution

When lowering a `CallExpr` that targets a generic function with concrete type args:
1. Mangle the name: `"{fn_name}__{type_arg}"` (e.g., `"max__Int"`)
2. Emit `Call { function: "max__Int", ... }`

The mangled name matches the `IrFunction.name` of the monomorphized instance.

---

## 5. The IR dump format

### 5.1 Rules

- **One instruction per line.** Two-space indentation per block depth.
- **Register format:** `%0`, `%1`, `%2`, ...
- **Block format:** `block_label:` on its own line, then indented instructions + terminator.
- **Function format:** `fn name(params) -> RetType {` then blocks then `}`.
- **Types shown on functions and params**, not on every instruction (the register map
  provides types).
- **Deterministic.** Same input ⇒ byte-identical output. No `{:?}`, no hashes.
- **LF only**, pinned via `.gitattributes` (`*.ir text eol=lf`).

### 5.2 Line grammar

```
fn <name>(<params>) -> <type> {
  <block_label>:
    %<n> = <instruction>
    <terminator>
}
```

### 5.3 Examples

```
fn main() -> Unit {
  entry:
    %0 = Call print [%1]
    %1 = Const String("Hello, Axiom!")
    Return Unit
}

fn add(let a: Int, let b: Int) -> Int {
  entry:
    %0 = BinOp + %a %b
    Return %0
}

fn max__Int(let a: Int, let b: Int) -> Int {  // monomorphized
  [generic_origin: max, concrete_args: [Int]]
  entry:
    %0 = BinOp > %a %b
    Branch %0 then_0 else_0
  then_0:
    Return %a
  else_0:
    Return %b
}
```

### 5.4 Kind labels — single source of truth

Kind names come from enum variant names. A `test_no_hardcoded_kind_labels` guard
scans the serializer source for quoted kind labels and fails if it finds any
(not counting the enum definition itself).

---

## 6. The six test layers

| Layer | What it is | The hole it closes |
|---|---|---|
| **1. Canonical dump** | One serializer `&Ir → String`, used by golden tests and CLI | "I can't see what the IR looks like" |
| **2. Golden snapshots** | `.ax` fixtures + `.ir` goldens, globbed by one test | "a change silently broke IR lowering" |
| **3. Coverage invariants** | Every Reg defined before use, every block has terminator, every jump target exists | **"a structural IR bug slipped through"** ← the core fear |
| **4. Diagnostics** | Ill-typed input → specific IR error + span, snapshotted | "an IR lowering error is silently swallowed" |
| **5. Fuzz / property** | Lower random-but-well-formed HIR; assert no panic, invariants hold | "the unimagined case" |
| **6. Unit tests** | Pinpoint checks (CFG structure, register allocation, monomorphized names) | "the subtle bug broad tests gloss over" |

Layers **3 and 5 are the load-bearing pair** — they make structural correctness mechanical.

### 6.1 Coverage invariants

```rust
pub fn check_invariants(ir: &Ir) -> Vec<String>
```

Checks:
1. **Register defined before use.** Every `Reg` referenced in an instruction or terminator
   must be defined by a prior instruction in the same function, or be an `IrParam` reg.
2. **Every block has exactly one terminator.** No block ends without a terminator; no block
   has multiple terminators.
3. **Every jump target exists.** Every `Jump`, `Branch`, `Match` target label must match
   a block label in the same function.
4. **Entry block is block 0.** The first block in `blocks` is the entry point.
5. **Every Call target exists.** Every `Call { function: "f", ... }` must reference a
   function name in `ir.functions`.
6. **No register reuse within a block.** Each `Reg` is assigned at most once per block.

Returns an empty vec if all invariants hold.

### 6.2 Golden test pattern

```rust
fn ir_source(source: &str) -> String {
    let result = axiom_parser::parse(source);
    let root = axiom_parser::ast::SourceFile::cast(result.tree).unwrap();
    let hir = axiom_hir::lower(&root, source);
    let thir = axiom_typeck::check(hir);
    let ir = axiom_ir::lower(&thir);
    axiom_ir::serialize(&ir)
}

fn check_golden(name: &str, source: &str) {
    let actual = ir_source(source);
    let golden_path = format!("tests/fixtures/{}.ir", name);
    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        std::fs::write(&golden_path, &actual).unwrap();
    } else {
        let expected = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|| panic!("golden file missing: {golden_path}"));
        assert_eq!(normalize(&actual), normalize(&expected), "golden mismatch for {name}");
    }
}
```

---

## 7. Roadmap

### v0 (this implementation)
- IR types (Reg, IrBlock, IrInstr, Terminator)
- HIR→IR lowering for non-generic programs
- IR serializer + golden tests
- Coverage invariants + fuzz

### v0 + generics step 7 (next)
- Wire `MonoResult` into IR lowering
- Clone function bodies with concrete type substitutions
- Emit monomorphized instances as separate `IrFunction`s
- Coverage invariant: every `MonoInstance` has a corresponding `IrFunction`

### v1 (ownership + RC)
- Ownership annotations on IR values (owned/borrowed)
- `incref`/`decref`/`reuse` instructions
- Ownership pass inserts RC operations
- IR snapshot tests for RC placement

### v1 (codegen)
- Cranelift backend: IR → native code
- IR interpreter: IR → WASM execution
- Shared FFI layer

---

## 8. Open questions

| # | Question | Status |
|---|---|---|
| 8.1 | Should registers carry type info in the IR, or is the TypeMap sufficient? | **[Decided]** Types on IrParam only; instruction result types inferred from the TypeMap or from the instruction itself. Registers don't carry types — the serializer looks them up. |
| 8.2 | How to handle `break` with a value in loop IR? | **[Deferred]** v0 loops are `Unit`-typed. Break-with-value (loop type inference) is implemented in the type checker but IR lowering defers to when the memory model lands. |
| 8.3 | Should the IR have explicit type annotations on every instruction for debuggability? | **[Decided]** No — too verbose. The serializer annotates functions and params; individual instructions show reg numbers only. A separate debug mode can show full type annotations. |
| 8.4 | How to represent `dyn Trait` in the IR? | **[Deferred]** v1.1. Will need vtable structs, fat pointers, indirect calls. |
