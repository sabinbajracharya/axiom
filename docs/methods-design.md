# Methods Design — impl Point { fn ... }

> **Status:** implementation in progress. Design is binding.
> **Decisions baked in:** receiver conventions (`let self` / `inout self` / `sink self`),
> dot-call syntax for methods (`p.dist(q)`), `::` for associated functions (`Point::new()`),
> methods are functions with a receiver — no special dispatch table, no vtable.
> **Prerequisites:** structs (§3.3), the convention system (§4.2). Traits are NOT required
> for inherent impls — this doc covers both inherent (`impl Point`) and trait (`impl Shape
> for Circle`) methods; the trait-specific concerns (bounds, completeness) are in
> [`traits-design.md`](traits-design.md).
> **Companion docs:** [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §3.3, §8.3,
> [`traits-design.md`](traits-design.md) (the polymorphism companion),
> [`ir-design.md`](ir-design.md) (the IR this lowers to),
> [`vm-design.md`](vm-design.md) (the VM that executes it),
> [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

---

## 0. The concern this answers

Axiom has structs and standalone functions, but no way to attach behavior to data.
Without methods:

- Every operation on a struct is a free function that takes the struct as a plain
  parameter — `dist(p, q)` instead of `p.dist(q)`.
- No namespace for operations — `area(circle)` and `area(rect)` collide.
- No `self` convention — the language can't express "this function borrows/mutates/
  consumes its receiver" as a first-class concept.
- No associated functions — `Point::new()` syntax is impossible.

Methods are not sugar over free functions. They are the mechanism that binds behavior
to types, enables dot-call syntax, and makes the convention system (`let`/`inout`/`sink`)
apply to the receiver.

---

## 1. The design, stated plainly

### 1.1 Inherent impl methods

```axiom
struct Point {
    x: Float,
    y: Float,
}

impl Point {
    // Associated function — no `self` param. Called via `::`
    fn origin() -> Point {
        Point { x: 0.0, y: 0.0 }
    }

    // Method — borrows self (read-only). Called via `.`
    fn dist(let self, other: Point) -> Float {
        let dx = self.x - other.x
        let dy = self.y - other.y
        dx + dy
    }

    // Method — mutates self in place. Called via `.`
    fn translate(inout self, dx: Float, dy: Float) {
        self.x = self.x + dx
        self.y = self.y + dy
    }

    // Method — consumes self. Called via `.`
    fn into_tuple(sink self) -> (Float, Float) {
        (self.x, self.y)
    }
}
```

### 1.2 Call syntax

```axiom
// Associated functions — `::` syntax
val origin = Point::origin()

// Methods — `.` syntax
val p = Point { x: 3.0, y: 4.0 }
val q = Point { x: 0.0, y: 0.0 }
val d = p.dist(q)         // let self — reads p

var p_mut = Point { x: 1.0, y: 2.0 }
p_mut.translate(10.0, 20.0)  // inout self — mutates p_mut

val consumed = p.into_tuple()  // sink self — consumes p
```

### 1.3 Receiver conventions

The receiver's convention is declared in the parameter list, just like any other
parameter. There is no `&self` / `&mut self` sugar — the convention system from §4.2
applies directly.

| Convention | Meaning | Source syntax | Example |
|---|---|---|---|
| `let self` | Read-only borrow | `fn foo(let self)` | `p.dist(q)` |
| `inout self` | Mutable borrow | `fn foo(inout self)` | `p.translate(1.0, 2.0)` |
| `sink self` | Consume | `fn foo(sink self)` | `p.into_tuple()` |

If no `self` parameter exists, the function is an **associated function** (not a method).
Associated functions are called via `::`, not `.`.

### 1.4 What this does NOT include

| Feature | Status | Why |
|---|---|---|
| Trait methods | See [`traits-design.md`](traits-design.md) | Separate concern — bounds, dispatch, defaults |
| Subscripts | See [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §4.4 | Different mechanism (suspend/resume/yield) |
| Operator overloading | Deferred → v1.1 | Requires `Add`/`Sub`/`Mul` traits |
| Method closures (`p.dist`) | Deferred → v2 | Requires first-class method references |
| Extension methods | Deferred → v2 | Add methods to foreign types — powerful but complex |
| `self` as a value (not receiver) | Deferred → v2 | `let x = self` inside a method body |

---

## 2. Type system impact

### 2.1 No new types

Methods do not add new variants to `Ty`. A method's type is `Ty::Fn(FnTy { ... })` —
the same as a standalone function. The receiver is just the first parameter.

The type checker's `impl_table` already stores `ImplInfo { type_name, methods, ... }`.
Method lookup resolves `receiver_ty` → type name → `impl_table` → method `FnDef`.

### 2.2 `self` type resolution

Inside `impl Point { ... }`, the `self` parameter has no explicit type annotation.
The type checker injects it synthetically:

1. `resolve_impl_self_type` looks up `Point` in the type env → `Ty::Struct(StructTy { name: "Point", ... })`.
2. `register_params` detects `param.name == "self"` and uses `current_self_type` instead
   of `param.ty`.
3. `self` gets `Ty::Struct(Point)` in the type map.

This is already implemented in `crates/axiom-typeck/src/typeck/mod.rs` lines 374-394.

### 2.3 Method dispatch in the type checker

`infer_method_call(mc: &MethodCallExpr)` in `crates/axiom-typeck/src/typeck/methods.rs`:

1. Infer receiver type.
2. Extract type name from receiver (`Ty::Struct → name`, `Ty::Enum → name`, etc.).
3. `find_impl_method(type_name, method_name, receiver_ty)` — searches inherent impls
   first, then trait impls.
4. `check_method_call` — filters out `self` param, applies substitution, checks arity + types.

---

## 3. IR representation

### 3.1 Method functions

An `impl Point { fn dist(let self, other: Point) -> Float { ... } }` produces an IR
function with a **qualified name** `"Point::dist"`:

```
fn Point::dist(self: Struct(Point), other: Struct(Point)) -> Float {
  entry:
  %2 = Field %0 x
  %3 = Field %1 x
  %4 = BinOp - %2 %3
  ...
  Return %n
}
```

The function name is the **qualified** method name (`Point::dist`). The `self` parameter
is the first parameter, with the struct type from the type checker.

### 3.2 Method call instructions

`p.dist(q)` lowers to:

```
%dst = MethodCall %p Point::dist [%q]
```

The `MethodCall` instruction stores:
- `dst: Reg` — destination register
- `receiver: Reg` — register holding the receiver value
- `method: String` — **qualified** method name (`"Type::method"`, e.g. `"Point::dist"`)
- `args: Vec<Reg>` — additional arguments (not including receiver)

The receiver is NOT in `args` — it is in `receiver`. The VM prepends it when building
the call frame.

### 3.3 Method name qualification

Method names are qualified as `"Type::method"` during IR lowering to prevent collisions
when two impls define the same method name:

```
impl Point { fn dist(...) { ... } }  →  fn Point::dist(...)
impl Vector { fn dist(...) { ... } }  →  fn Vector::dist(...)
```

The qualification happens in two places:
1. **`lower_item`** (`crates/axiom-ir/src/lower/item.rs`) — when registering impl
   methods, extracts the type name from `impl_def.type_name` and passes it as a
   prefix to `lower_fn_def`.
2. **`lower_method_call`** (`crates/axiom-ir/src/lower/expr.rs`) — when emitting a
   `MethodCall` instruction, looks up the receiver type from `ctx.types` and
   constructs the qualified name.

Builtin functions (`print`, `println`) are unaffected — they are not methods on
struct/enum types and remain bare names.

### 3.3 Associated function calls

`Point::origin()` lowers to a plain `Call` instruction (not `MethodCall`):

```
%dst = Call origin []
```

Associated functions have no receiver — they are just regular function calls.

### 3.4 Field access

`self.x` lowers to:

```
%dst = Field %self x
```

The `Field` instruction reads a named field from a struct value.

---

## 4. VM execution

### 4.1 MethodCall handler

In `crates/axiom-vm/src/exec/instr.rs`:

```rust
IrInstr::MethodCall { dst, receiver, method, args } => {
    let recv = self.read_reg(receiver)?;
    let mut all_args = vec![recv];          // receiver becomes `self`
    for r in &args { all_args.push(read_reg(r)?); }

    if is_builtin(&method) {               // builtin fast path
        return call_builtin(&method, all_args);
    }

    self.push_frame(&method, all_args)?;    // dispatch by bare name
}
```

The VM looks up the method by bare name in `fn_map: HashMap<String, usize>`.
For single-impl scenarios (one type defining a method name), this works directly.

### 4.2 Function lookup

`fn_map` is built from `Ir::functions` in `Vm::new()`. Each function is indexed by its
`IrFunction::name`. For impl methods, the name is the bare method name (e.g., `"dist"`).

**Known limitation:** If two different impls define the same method name (e.g.,
`impl Circle { fn area(...) }` and `impl Rect { fn area(...) }`), the IR will have two
functions both named `"area"` — collision. This is a v0 trade-off; qualified names
(`"Circle.area"`) are the v1 fix.

---

## 5. Name resolution

### 5.1 Dot-call resolution

`obj.method(args)` is parsed as `MethodCallExpr` (not `CallExpr`). Name resolution
resolves the receiver expression and arguments, but NOT the method name — the method
name is resolved during type checking (when the receiver's type is known).

### 5.2 Colon-colon resolution

`Type::assoc_fn(args)` is parsed as a `CallExpr` with a path callee. Name resolution
resolves the path to the function's `DefId`.

### 5.3 Impl method names in name resolution

Impl method names are NOT registered in the top-level scope. `dist` is not a standalone
name — it is only accessible via `p.dist()`. This prevents `dist(p)` from accidentally
resolving to the impl method.

---

## 6. Cross-pipeline fixture invariant

Every feature that has a fixture in ANY pipeline stage must have a fixture in ALL stages.
The `methods` feature must have fixtures in:

| Stage | Fixture | Golden | Tests |
|---|---|---|---|
| Parser | `methods.ax` | `methods.ast` | CST structure: `ImplBlock`, `MethodCallExpr`, `FieldExpr` |
| HIR | `methods.ax` | `methods.hir` | `Item::ImplDef`, `Expr::MethodCall`, `Expr::Field` |
| Typeck | `methods.ax` | `methods.thir` | receiver type resolved, `self` typed, method dispatch |
| IR | `methods.ax` | `methods.ir` | `fn dist(self: ...)`, `MethodCall` instruction |
| VM | `methods.ax` | `methods.trace` | MethodCall dispatch, receiver as first arg, correct output |

The `traits` feature must also be consistent (all stages have `traits.ax` with
`struct Circle` defined).

---

## 7. Implementation checklist

### Phase 1: Fix broken fixtures

- [x] Fix `crates/axiom-typeck/tests/fixtures/traits.ax` — add `struct Circle { r: Float }`
- [x] Fix `crates/axiom-parser/tests/fixtures/traits.ax` — add `struct Circle { r: Float }`
- [x] Fix `crates/axiom-ir/tests/fixtures/traits.ax` — add `struct Circle { r: Float }` + `fn main()`

### Phase 2: Update `methods` fixtures to test actual impl methods

- [x] Update `crates/axiom-parser/tests/fixtures/methods.ax` — struct + impl + method call
- [x] Update `crates/axiom-hir/tests/fixtures/methods.ax` — same
- [x] Update `crates/axiom-typeck/tests/fixtures/methods.ax` — same
- [x] Update `crates/axiom-ir/tests/fixtures/methods.ax` — struct + impl + main
- [x] Update `crates/axiom-vm/tests/fixtures/methods.ax` — struct + impl + main + print

### Phase 3: Regenerate goldens

- [x] Regenerate `traits.ast` (parser golden)
- [x] Regenerate `traits.thir` (typeck golden)
- [x] Regenerate `traits.ir` (IR golden)
- [x] Regenerate `traits.trace` (VM golden)
- [x] Regenerate `methods.ast` (parser golden)
- [x] Regenerate `methods.hir` (HIR golden)
- [x] Regenerate `methods.thir` (typeck golden)
- [x] Regenerate `methods.ir` (IR golden)
- [x] Regenerate `methods.trace` (VM golden)

### Phase 4: Verify

- [x] All golden tests pass (`cargo test` in each crate)
- [x] Fixture coverage test passes (`cargo test -p axiom-cli -- test_fixture_coverage`)
- [x] Pre-commit gate passes (`cargo fmt && cargo clippy -D warnings && cargo test`)
- [x] `self` param has correct type in IR (not `<error>`)
- [x] MethodCall instruction appears in IR output for `.method()` calls
- [x] VM trace shows method body execution (not just `main` entry)

### Phase 5: Qualify method names in IR

- [x] `lower_item` passes impl `type_name` to `lower_fn_def` as prefix
- [x] `lower_fn_def` registers methods as `"Type::method"` (e.g., `"Point::dist"`)
- [x] `lower_method_call` looks up receiver type, emits qualified name in MethodCall instruction
- [x] VM `push_frame` resolves qualified name via `fn_map` (no VM changes needed)
- [x] Regenerate `methods.ir` and `methods.trace` golden files
- [x] Regenerate `traits.ir` and `traits.trace` golden files
- [x] Pre-commit gate passes

---

## 8. Known limitations (v0)

| Limitation | Impact | Fix |
|---|---|---|
| ~~Bare method name in IR (`"dist"`, not `"Point.dist"`)~~ | ~~Collision if two impls define same method name~~ | **Resolved** — methods now register as `"Type::method"` |
| No method resolution via trait bounds | `fn foo<T: Shape>(s: T) { s.area() }` won't work | Requires monomorphization of trait dispatch |
| No `Self` type in inherent impls | `fn new() -> Self` not supported | Add `Self` alias in type checker |
| No method overloading | `fn dist(let self)` and `fn dist(let self, other: Point)` can't coexist | Axiom has singular idiom — not planned |
| No computed property access | `p.magnitude` (computed, not stored) | Requires property syntax or `subscript` |
