# Generics Design — Parametric, Bounded, Monomorphized

> **Status:** authoritative for the generics implementation. Binding before code is written.
> **Decisions baked in:** plain parametric generics with trait bounds, monomorphization
> (like Rust/C++ templates), no associated types / HKTs / const generics / variance in v1.
> **Companion docs:** [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §3.6, §3.7,
> [`traits-design.md`](traits-design.md) (the companion — traits),
> [`collection-type-design.md`](collection-type-design.md) (the consumer — collections),
> [`typeck-testing.md`](typeck-testing.md) (the layer this extends),
> [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

---

## 0. The concern this answers

The current type system (`Ty` in `crates/axiom-typeck/src/types.rs`) has no concept of
type parameters. `StructTy` is `{ name, def_id }` — no type arguments. There's no way to
express `List<Int>`, `Pair<String, Bool>`, or `fn max<T: Ord>(a: T, b: T) -> T`.

Without generics, every collection must be monomorphic (one `List` per element type),
every utility function must be duplicated per type, and the library-type collection design
(`docs/collection-type-design.md`) is unimplementable.

The fear: **we bake a non-generic type system into the compiler and then retrofit generics
onto it**, which means rewriting the type checker, HIR, THIR serializer, and IR generator.
This doc pins the design so generics are a planned extension, not an afterthought.

---

## 1. The design, stated plainly

### 1.1 What generics add to the language

```axiom
// Generic functions
fn identity<T>(let x: T) -> T { x }
fn max<T: Ord>(let a: T, let b: T) -> T { if a > b { a } else { b } }

// Generic structs
struct Pair<A, B> { first: A, second: B }
struct List<T: Deinit> { buffer: DynamicBuffer<Int, T> }

// Generic enums
enum Option<T> { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }

// Using generic types
val p: Pair<Int, String> = Pair { first: 1, second: "hi" }
val xs: List<Int> = [1, 2, 3]
val n: Option<Int> = Option.Some(42)
```

### 1.2 What v1 explicitly does NOT include

| Feature | Status | Why |
|---------|--------|-----|
| Associated types | `[Deferred → v2]` | Adds complexity to trait resolution; not needed for collections |
| Higher-kinded types | `[Deferred → v2]` | Research-language territory; not needed for v1 |
| Const generics | `[Deferred → v2]` | Needed for `Array<N, T>` fixed-size arrays; not needed for v1 |
| Variance (covariance/contravariance) | `[Deferred → v2]` | Subtle; v1 uses invariance (simplest, sound) |
| Generic impls (`impl<T> Foo<T>`) | **Included** | Required for `impl<T: Deinit> List<T>` |
| Trait bounds on generic params | **Included** | Required for `fn max<T: Ord>` |

### 1.3 The pipeline impact

Generics touch every stage of the compiler:

```
Source:     fn max<T: Ord>(let a: T, let b: T) -> T { ... }
                    │
Parser:     ParseTypeParams → [TypeParam { name: "T", bounds: [Ord] }]
                    │
HIR:        HirItem::Fn { type_params: [TypeParamId(0)], ... }
                    │
Type check: Instantiate T with concrete types at each call site
                    │
Monomorph:  Generate max_Int, max_Float, max_String (one per concrete type)
                    │
IR:         Each monomorphized instance is a separate IR function
                    │
Codegen:    Each instance gets its own machine code (Cranelift)
```

---

## 2. Type system changes

### 2.1 New types in `Ty`

The `Ty` enum gains two new variants:

```rust
pub enum Ty {
    // ... existing variants ...

    /// A generic type parameter, used during type checking before monomorphization.
    /// After monomorphization, all TypeParam variants are replaced with concrete types.
    TypeParam(TypeParamId),

    /// A concrete instance of a generic type: `List<Int>`, `Pair<String, Bool>`.
    /// This is what exists after monomorphization. Before monomorphization,
    /// generic types are represented as Struct/Enum with TypeParam fields.
    Instance(InstanceTy),
}
```

### 2.2 `TypeParamId`

```rust
/// Identifies a type parameter within its enclosing scope.
/// The `index` is the 0-based position in the type parameter list.
/// `name` is for display only (e.g., "T", "U", "K").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeParamId {
    pub name: String,
    pub index: usize,
    pub def_id: DefId,  // points to the defining item (fn, struct, trait)
}
```

### 2.3 `InstanceTy`

```rust
/// A concrete instantiation of a generic type.
/// `List<Int>` = InstanceTy { def_id: List's DefId, args: [Ty::Int] }
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceTy {
    pub name: String,
    pub def_id: DefId,
    pub args: Vec<Ty>,
}
```

### 2.4 `GenericParams` — the type parameter list

```rust
/// The type parameters declared on a generic item.
#[derive(Debug, Clone, PartialEq)]
pub struct GenericParams {
    pub params: Vec<TypeParam>,
}

/// A single type parameter with optional trait bounds.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: String,
    pub def_id: DefId,
    pub bounds: Vec<TraitBound>,
}

/// A trait bound on a type parameter: `T: Ord` → TraitBound { trait_name: "Ord", trait_def_id: ... }
#[derive(Debug, Clone, PartialEq)]
pub struct TraitBound {
    pub trait_name: String,
    pub trait_def_id: DefId,
}
```

### 2.5 Changes to existing types

```rust
// Before (current)
pub struct StructTy {
    pub name: String,
    pub def_id: DefId,
}

// After (with generics)
pub struct StructTy {
    pub name: String,
    pub def_id: DefId,
    pub generic_params: GenericParams,  // declared type params (empty for non-generic structs)
}
```

Same for `EnumTy` and `FnTy` — they gain a `generic_params` field.

### 2.6 How `Pair<Int, String>` is represented

**Before monomorphization** (in the THIR after type checking):
```
Ty::Instance(InstanceTy {
    name: "Pair",
    def_id: DefId(...),
    args: [Ty::Int, Ty::String],
})
```

**After monomorphization** (in the IR): the same `InstanceTy` — monomorphization doesn't
change the type representation, it generates specialized code for each unique `InstanceTy`.

### 2.7 Display format

```
Pair<Int, String>         → "Pair<Int, String>"
List<Int>                 → "List<Int>"
Option<Option<Int>>       → "Option<Option<Int>>"
fn<T>(T) -> T             → "fn<T>(T) -> T"
T (unresolved param)      → "T"
T: Ord (bounded param)    → "T: Ord"
```

---

## 3. Parser changes

### 3.1 Type parameter syntax

```ebnf
TypeParams      = "<" TypeParam ("," TypeParam)* ">" ;
TypeParam       = IDENT (":" TraitBound ("+" TraitBound)*)? ;
TraitBound      = PathExpr ;
```

Examples:
```
<T>
<T: Ord>
<T: Hashable + Equatable>
<A, B>
<T: Deinit, U: Display>
```

### 3.2 Where type parameters appear

| Context | Syntax | Example |
|---------|--------|---------|
| Function declaration | `fn name<T>(...)` | `fn max<T: Ord>(a: T, b: T) -> T` |
| Struct declaration | `struct Name<T>` | `struct Pair<A, B> { first: A, second: B }` |
| Enum declaration | `enum Name<T>` | `enum Option<T> { Some(T), None }` |
| Trait declaration | `trait Name<T>` | (not in v1 — traits are not themselves generic) |
| Type annotation | `Name<ConcreteType>` | `val x: Pair<Int, String> = ...` |
| turbofish (fn call) | `name::<ConcreteType>(...)` | `identity::<Int>(42)` |

### 3.3 The turbofish problem

When calling a generic function, the type arguments may need to be specified explicitly
if they can't be inferred:

```axiom
fn identity<T>(let x: T) -> T { x }

val a = identity(42)          // T inferred as Int — OK
val b = identity::<Int>(42)   // explicit turbofish — also OK
```

The parser must support `::<...>` after a function name in call position. This is the
turbofish syntax (Rust uses the same).

### 3.4 Grammar changes

Add to the existing grammar:

```ebnf
FnDecl          = "fn" IDENT TypeParams? "(" ParamList ")" ("->" Type)? Block ;
StructDecl      = "struct" IDENT TypeParams? StructBody ;
EnumDecl        = "enum" IDENT TypeParams? "{" EnumVariants "}" ;
TypePath        = IDENT ("<" Type ("," Type)* ">")? ;
FnCall          = Expr TypeParams? "(" ArgList ")" ;  // TypeParams for turbofish
```

---

## 4. HIR changes

### 4.1 New HIR nodes

```rust
/// A type parameter declaration.
#[derive(Debug, Clone)]
pub struct HirTypeParam {
    pub id: HirId,
    pub name: String,
    pub bounds: Vec<TraitBound>,
    pub span: Span,
}

/// A type argument in a usage site: `List<Int>` → TypeArg { ty: Ty::Int }
#[derive(Debug, Clone)]
pub struct HirTypeArg {
    pub id: HirId,
    pub ty: Ty,
    pub span: Span,
}
```

### 4.2 Changes to existing HIR items

```rust
// HirItem::Fn gains type_params
HirItem::Fn {
    id: HirId,
    name: String,
    type_params: Vec<HirTypeParam>,  // NEW
    params: Vec<HirParam>,
    return_type: Option<HirType>,
    body: HirBlock,
    span: Span,
}

// HirItem::Struct gains type_params
HirItem::Struct {
    id: HirId,
    name: String,
    type_params: Vec<HirTypeParam>,  // NEW
    fields: Vec<HirField>,
    span: Span,
}

// HirItem::Enum gains type_params
HirItem::Enum {
    id: HirId,
    name: String,
    type_params: Vec<HirTypeParam>,  // NEW
    variants: Vec<HirVariant>,
    span: Span,
}
```

### 4.3 Type resolution changes

When the resolver encounters `List<Int>`:
1. Resolve `List` to its `DefId`.
2. Check that `List` has 1 type parameter.
3. Resolve `Int` to `Ty::Int`.
4. Construct `Ty::Instance(InstanceTy { name: "List", def_id, args: [Ty::Int] })`.

When the resolver encounters `T` inside a generic function:
1. Look up `T` in the current type parameter scope.
2. Construct `Ty::TypeParam(TypeParamId { name: "T", index: 0, def_id })`.

---

## 5. Type checker changes

### 5.1 Type parameter scoping

The type checker maintains a **type parameter scope** — a stack of `TypeParamId → TraitBound`
mappings. When entering a generic function body, the function's type parameters are pushed.
When leaving, they're popped.

```rust
struct TypeParamScope {
    /// Stack of type parameter definitions. The outermost scope is at index 0.
    scopes: Vec<Vec<(TypeParamId, Vec<TraitBound>)>>,
}

impl TypeParamScope {
    fn push(&mut self, params: &[TypeParam]) { ... }
    fn pop(&mut self) { ... }
    fn lookup(&self, name: &str) -> Option<&(TypeParamId, Vec<TraitBound>)> { ... }
}
```

### 5.2 Type instantiation

When type-checking a call to a generic function:

```axiom
fn max<T: Ord>(let a: T, let b: T) -> T { ... }
val result = max(3, 5)
```

1. Collect the function's type parameters: `[T: Ord]`.
2. For each call argument, infer its type: `3 → Int`, `5 → Int`.
3. Unify each type parameter with the inferred type: `T = Int`.
4. Check that the concrete type satisfies the bounds: `Int: Ord` → yes (if `Ord` is implemented for `Int`).
5. Substitute `T → Int` in the return type: `T → Int`.
6. The call expression gets type `Int`.

### 5.3 Unification with type parameters

When unifying a type parameter `T` with a concrete type `Int`:
- If `T` is not yet bound: bind `T → Int`.
- If `T` is already bound to `Int`: success (same type).
- If `T` is already bound to `Float`: error (type mismatch).

When unifying two type parameters `T` and `U`:
- If neither is bound: bind `T → U` (or `U → T`, pick one consistently).
- If one is bound: bind the other to the same type.
- If both are bound to different types: error.

### 5.4 Bound checking

After all type parameters are resolved to concrete types, check that each concrete type
satisfies its bounds:

```
T: Ord, resolved to Int → check: is there an `impl Ord for Int`? → yes/no
T: Hashable + Equatable, resolved to String → check: both impls exist?
```

If a bound is not satisfied, emit a diagnostic:
```
error: the type `Int` does not implement the trait `Hashable`
  --> source.ax:5:10
  |
5 | val s: Set<Int> = {1, 2, 3}
  |          ^^^ `Int` missing `Hashable` impl
```

### 5.5 Type inference for generic calls

Type inference proceeds bidirectionally:

1. **Bottom-up (infer):** infer argument types from their expressions.
2. **Top-down (check):** if the expected type is known (e.g., from a type annotation),
   use it to resolve type parameters.
3. **Unify:** match inferred types against parameter types to determine type arguments.

```axiom
val x: Pair<Int, String> = Pair { first: 1, second: "hi" }
//    ^ expected type: Pair<Int, String>
//    → A = Int, B = String
//    → check first: Int matches 1's inferred type → OK
//    → check second: String matches "hi"'s inferred type → OK
```

---

## 6. Monomorphization

### 6.1 What it is

Monomorphization generates a specialized copy of each generic function for each unique
combination of concrete type arguments. The result is zero-cost — no boxing, no vtables,
no runtime type dispatch.

```
fn max<T: Ord>(a: T, b: T) -> T { ... }

// Used as:
max(3, 5)        → generates max__Int(a: Int, b: Int) -> Int
max(1.5, 2.5)    → generates max__Float(a: Float, b: Float) -> Float
max("a", "b")    → generates max__String(a: String, b: String) -> String
```

### 6.2 When it happens

Monomorphization happens **after type checking, before IR generation**. The flow is:

```
Parse → HIR → Resolve → Type Check → Monomorphize → IR → Codegen
```

The type checker produces a THIR with `Ty::Instance(...)` for all generic usages.
The monomorphizer walks the THIR and:

1. Collects all unique `InstanceTy` values (e.g., `List<Int>`, `List<String>`).
2. For each, specializes the generic definition by substituting type parameters.
3. Replaces generic calls with calls to the specialized instances.
4. Emits a dependency graph: `max__Int` depends on nothing, `List<Int>::push` depends on
   `DynamicBuffer<Int>` allocation, etc.

### 6.3 The instantiation table

```rust
/// Tracks all monomorphized instances generated during compilation.
struct InstantiationTable {
    /// Maps (generic_def_id, concrete_args) → monomorphized_def_id.
    instances: HashMap<(DefId, Vec<Ty>), DefId>,
    /// Queue of instances to generate.
    worklist: Vec<(DefId, Vec<Ty>)>,
}
```

### 6.4 Deduplication

Each unique `(DefId, Vec<Ty>)` pair generates exactly one specialized function. If
`List<Int>::push` is called from 10 call sites, only one `push__List_Int` is generated.

### 6.5 Code bloat

Monomorphization increases code size. This is the documented cost of zero-cost
abstractions (§1.5 complexity budget). Mitigations:
- Deduplication (§6.4).
- The compiler may inline small generic functions (e.g., `identity<T>`).
- Post-v1: profile-guided optimization can prune unused instances.
- The `dyn Trait` escape hatch avoids monomorphization when code size matters more
  than performance (§3.5 — dynamic dispatch, opt-in).

### 6.6 Naming convention for monomorphized functions

```
<original_name>__<type_arg_1>_<type_arg_2>_...
```

Examples:
```
max__Int
max__Float
identity__String
List__Int__push
List__String__pop
```

---

## 7. IR changes

### 7.1 Generic functions in the IR

Before monomorphization, the IR has generic function definitions with type parameters.
After monomorphization, each instance is a concrete IR function with no type parameters.

```rust
// Before monomorphization
IrFunction {
    name: "max",
    type_params: [TypeParamId { name: "T", ... }],
    params: [IrParam { name: "a", ty: Ty::TypeParam(...) }, ...],
    return_type: Ty::TypeParam(...),
    body: ...,
}

// After monomorphization
IrFunction {
    name: "max__Int",
    type_params: [],  // empty — fully concrete
    params: [IrParam { name: "a", ty: Ty::Int }, ...],
    return_type: Ty::Int,
    body: ...,
}
```

### 7.2 Instantiation metadata

The IR includes metadata linking monomorphized instances back to their generic origin:

```rust
IrFunction {
    name: "max__Int",
    generic_origin: Some(GenericOrigin {
        generic_def_id: DefId(...),
        concrete_args: [Ty::Int],
    }),
    ...
}
```

This is used for diagnostics (error messages point to the generic definition, not the
monomorphized copy) and for debugging.

---

## 8. THIR dump format changes

### 8.1 Type parameter declarations

```
Fn(max) type_params=[T: Ord]
  Param(a) : T
  Param(b) : T
  Return : T
  ...
```

### 8.2 Generic type usage

```
Struct(Pair) type_params=[A, B]
  Field(first) : A
  Field(second) : B

// Usage:
StructExpr(Pair<Int, String>)
  FieldInit(first) : Int
  FieldInit(second) : String
```

### 8.3 Monomorphized instances

After monomorphization, the THIR dump shows the specialized versions:

```
Fn(max__Int) generic_origin=max<T=Int>
  Param(a) : Int
  Param(b) : Int
  Return : Int
  ...
```

---

## 9. Testing spec

### 9.1 The six layers

| Layer | What it is | The hole it closes |
|---|---|---|
| **1. Canonical dump** | THIR dump shows type params, type args, and monomorphized instances | "I can't see what generic resolution produced" |
| **2. Golden snapshots** | `.ax` fixtures + checked-in `.thir` goldens with generic types | "a change silently broke generic resolution" |
| **3. Coverage invariants** | Every type param is resolved; every bound is checked; every instance is monomorphized | **"a generic type param left unresolved"** |
| **4. Diagnostics** | Unresolved type param, unsatisfied bound, type mismatch → specific error + span | "a generic error is silently accepted" |
| **5. Fuzz / property** | Random generic programs; assert no panic, all params resolved, all bounds checked | "the unimagined case" |
| **6. Unit tests** | Pinpoint checks on unification, bound checking, monomorphization dedup | "the subtle generic bug broad tests gloss over" |

### 9.2 Coverage invariants

#### 9.2.1 `type_params_resolved(thir) -> Result<(), GenericCoverageError>`

Asserts that every `Ty::TypeParam` in the THIR is either:
- Inside a generic definition (OK — it's a declaration), or
- Resolved to a concrete type by the end of type checking (OK — it's been substituted).

If any `Ty::TypeParam` survives past type checking without being resolved, this invariant
fails.

#### 9.2.2 `bounds_checked(thir) -> Result<(), BoundCheckError>`

Asserts that for every type parameter with trait bounds, the concrete type it's resolved
to actually implements those traits. This is the bound-checking pass's correctness proof.

#### 9.2.3 `instances_complete(thir) -> Result<(), InstanceError>`

Asserts that for every `Ty::Instance(def_id, args)` in the THIR, a monomorphized
function exists in the IR. If any instance is missing its monomorphized code, this
invariant fails.

#### 9.2.4 `monomorphization_terminates(ir) -> Result<(), TerminationError>`

Asserts that the monomorphization worklist eventually empties. This catches infinite
instantiation cycles (e.g., a generic function that indirectly instantiates itself
with the same type args — which is actually fine, but with different type args in a
cycle could be infinite).

### 9.3 Golden snapshot fixtures

#### Parser fixtures

| Fixture | Tests |
|---|---|
| `generic_fn_decl.ax` | `fn identity<T>(let x: T) -> T { x }` — parses type params |
| `generic_fn_with_bounds.ax` | `fn max<T: Ord>(...)` — parses trait bounds |
| `generic_fn_multi_params.ax` | `fn convert<A, B>(...)` — multiple type params |
| `generic_struct_decl.ax` | `struct Pair<A, B> { ... }` — struct type params |
| `generic_enum_decl.ax` | `enum Option<T> { ... }` — enum type params |
| `turbofish_call.ax` | `identity::<Int>(42)` — turbofish syntax |
| `generic_type_annotation.ax` | `val x: Pair<Int, String> = ...` — type args in annotations |
| `nested_generic_type.ax` | `val x: Option<Option<Int>>` — nested type args |

#### Type checker fixtures

| Fixture | Tests |
|---|---|
| `generic_fn_infer.ax` | `max(3, 5)` — T inferred as Int from arguments |
| `generic_fn_check.ax` | `val x: Int = max(3, 5)` — T resolved from expected type |
| `generic_fn_turbofish.ax` | `max::<Int>(3, 5)` — T resolved from turbofish |
| `generic_struct_infer.ax` | `Pair { first: 1, second: "hi" }` — A, B inferred |
| `generic_struct_check.ax` | `val p: Pair<Int, String> = ...` — A, B from annotation |
| `generic_enum_infer.ax` | `Option.Some(42)` — T inferred as Int |
| `bound_satisfied.ax` | `max(3, 5)` with `Int: Ord` — bound check passes |
| `bound_unsatisfied.ax` | `max(a, b)` where type doesn't impl Ord — bound check fails |
| `type_mismatch_in_generic.ax` | `Pair { first: 1, second: 2 }` with expected `Pair<Int, String>` — error |
| `unresolved_type_param.ax` | `T` used outside generic context — error |

#### Monomorphization fixtures

| Fixture | Tests |
|---|---|
| `mono_single_instance.ax` | `max(3, 5)` — generates one `max__Int` |
| `mono_dedup.ax` | `max(3, 5); max(1, 2)` — one `max__Int`, not two |
| `mono_multi_instance.ax` | `max(3, 5); max(1.5, 2.5)` — `max__Int` and `max__Float` |
| `mono_nested_generic.ax` | `identity(max(3, 5))` — monomorphizes both |
| `mono_generic_struct.ax` | `Pair<Int, String>` — generates specialized struct layout |
| `mono_generic_enum.ax` | `Option<Int>.Some(42)` — generates specialized enum |

#### Diagnostic fixtures

| Fixture | Tests |
|---|---|
| `err_unsatisfied_bound.ax` | Missing trait impl → clear error with span |
| `err_wrong_type_arg_count.ax` | `List<Int, String>` when List has 1 param → error |
| `err_type_mismatch_in_generic.ax` | Type mismatch inside generic body → error |
| `err_turbofish_mismatch.ax` | `identity::<Int>("hello")` → error |

### 9.4 Fuzz / property tests

#### 9.4.1 `fuzz_generic_programs(seed) -> FuzzResult`

Generate random programs with generic functions, structs, and enums. Assert:
- Parser never panics.
- Type checker resolves all type parameters or emits diagnostics.
- Monomorphizer terminates (worklist empties).
- No `Ty::TypeParam` survives past monomorphization.
- Every `Ty::Instance` has a corresponding monomorphized function.

#### 9.4.2 `fuzz_unification(seed) -> FuzzResult`

Generate random unification problems (type param vs concrete type, type param vs type
param, nested generics). Assert:
- Unification terminates.
- Result is either a consistent substitution or a clear error.
- No contradictory bindings (T=Int and T=Float simultaneously).

#### 9.4.3 `fuzz_bound_checking(seed) -> FuzzResult`

Generate random trait bound scenarios. Assert:
- Bound checking terminates.
- Satisfied bounds pass, unsatisfied bounds produce diagnostics.
- No false positives (accepting a type that doesn't satisfy a bound).

### 9.5 Unit tests

#### Parser

| Test | What it verifies |
|---|---|
| `test_parse_type_params_empty` | `fn foo()` — no type params |
| `test_parse_type_params_single` | `fn foo<T>()` — one type param |
| `test_parse_type_params_with_bound` | `fn foo<T: Ord>()` — bound parsing |
| `test_parse_type_params_multi_bound` | `fn foo<T: Hashable + Equatable>()` — multiple bounds |
| `test_parse_type_params_multi` | `fn foo<A, B, C>()` — multiple params |
| `test_parse_turbofish` | `foo::<Int>(42)` — turbofish in call |
| `test_parse_generic_type_annotation` | `val x: List<Int>` — type args in annotation |
| `test_parse_nested_generic` | `val x: Option<Option<Int>>` — nesting |

#### Type checker — resolution

| Test | What it verifies |
|---|---|
| `test_resolve_type_param_in_body` | `T` resolves inside generic fn body |
| `test_resolve_type_param_in_annotation` | `T` resolves in return type annotation |
| `test_resolve_generic_struct_usage` | `Pair<Int, String>` resolves to InstanceTy |
| `test_resolve_generic_enum_usage` | `Option<Int>` resolves to InstanceTy |
| `test_resolve_turbofish` | `::<Int>` overrides inference |

#### Type checker — unification

| Test | What it verifies |
|---|---|
| `test_unify_type_param_with_concrete` | `T = Int` succeeds |
| `test_unify_type_param_with_type_param` | `T = U` succeeds |
| `test_unify_concrete_with_concrete_same` | `Int = Int` succeeds |
| `test_unify_concrete_with_concrete_diff` | `Int = Float` fails |
| `test_unify_already_bound_same` | `T = Int, T = Int` succeeds |
| `test_unify_already_bound_diff` | `T = Int, T = Float` fails |
| `test_unify_nested_generic` | `Option<T> = Option<Int>` → T = Int |

#### Type checker — bound checking

| Test | What it verifies |
|---|---|
| `test_bound_satisfied` | `T: Ord, T=Int, Int: Ord` → pass |
| `test_bound_unsatisfied` | `T: Ord, T=Foo, Foo: !Ord` → error |
| `test_multiple_bounds_all_satisfied` | `T: Hashable + Equatable, T=String` → pass |
| `test_multiple_bounds_one_missing` | `T: Hashable + Equatable, T=Foo` where only `Hashable` → error |

#### Monomorphization

| Test | What it verifies |
|---|---|
| `test_mono_single_instance` | One call site → one instance |
| `test_mono_dedup` | Two call sites, same types → one instance |
| `test_mono_multi_instance` | Two call sites, different types → two instances |
| `test_mono_terminates` | Recursive generic → worklist terminates |
| `test_mono_no_type_params_in_output` | No `Ty::TypeParam` in monomorphized IR |
| `test_mono_all_instances_generated` | Every `InstanceTy` has a function |

---

## 10. Implementation order

1. **Parser:** type params on fn/struct/enum decl, type args in annotations, turbofish.
2. **HIR:** `HirTypeParam`, `HirTypeArg`, `GenericParams` on items.
3. **Name resolution:** type param scoping, type arg resolution.
4. **Type checker:** `TypeParam` variant in `Ty`, unification with type params, bound checking.
5. **Monomorphizer:** instantiation table, specialization, worklist.
6. **IR:** generic function representation, monomorphized instances.
7. **THIR dump:** show type params, type args, monomorphized instances.
8. **Tests:** golden snapshots, coverage invariants, fuzz, unit tests.

Steps 1-4 are the "generic type checking" milestone. Steps 5-6 are the "monomorphization"
milestone. Step 7-8 run in parallel with everything.

---

## 11. Honest open questions

| # | Question | Status |
|---|----------|--------|
| 1 | **Turbofish syntax conflicts** — does `::<` conflict with any existing grammar? | **Open** — needs grammar analysis |
| 2 | **Recursive generic types** — `struct Foo<T> { next: Option<Foo<T>> }` — is this infinite? | **Open** — needs termination check; Rust allows this via indirection |
| 3 | **Generic enum variant inference** — `Some(42)` infers `Option<Int>` or needs context? | **Open** — Rust requires context; Axiom should match |
| 4 | **Where clause syntax** — `fn foo<T>(...) where T: Ord` vs inline `fn foo<T: Ord>(...)`? | **Decided: inline only** — singular idiom, one way to write bounds |
| 5 | **Implicit type param bounds** — should `T` implicitly satisfy `Deinit`? | **Open** — if yes, every type param is destructible (likely correct) |
