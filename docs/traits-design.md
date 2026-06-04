# Traits Design — The Only Polymorphism

> **Status:** authoritative for the traits implementation. Binding before code is written.
> **Decisions baked in:** traits define behavior (not state), static dispatch by default
> (monomorphized), `dyn Trait` for dynamic dispatch (opt-in, visible cost), orphan rule
> for coherence, default methods allowed.
> **Prerequisites:** generics (§3.6) — traits require generic type parameters for bounded
> polymorphism. **Implement after or in parallel with [`generics-design.md`](generics-design.md).**
> **Companion docs:** [`DESIGN_SPEC.md`](../DESIGN_SPEC.md) §3.5,
> [`generics-design.md`](generics-design.md) (the companion — generics),
> [`collection-type-design.md`](collection-type-design.md) (the consumer — collections need `Deinit`, `Hashable`, `Equatable`),
> [`typeck-testing.md`](typeck-testing.md) (the layer this extends),
> [`RUST_CONVENTIONS.md`](../RUST_CONVENTIONS.md), [`ENFORCEMENT.md`](../ENFORCEMENT.md).

---

## 0. The concern this answers

The current type system has no concept of shared behavior across types. Every function
operates on concrete types. There's no way to write `fn max<T: Ord>(a: T, b: T) -> T`
because there's no `Ord` trait, no trait bounds, and no `impl` blocks.

Without traits, generic functions can't constrain their type parameters, the collection
type design (`List<T: Deinit>`, `Map<K: Hashable + Equatable, V: Deinit>`) is
unimplementable, and the language has no mechanism for ad-hoc polymorphism (operator
overloading, custom equality, custom hashing, custom ordering).

The fear: **we build a type system where every function is monomorphic or has unconstrained
type parameters**, which means no type-safe generic collections, no operator overloading,
and no way to write reusable code that works across types.

---

## 1. The design, stated plainly

### 1.1 What traits add to the language

```axiom
// Declaration
trait Shape {
    fn area(let self) -> Float;
    fn name(let self) -> String { "shape" }   // default method
}

// Implementation
impl Shape for Circle {
    fn area(let self) -> Float { 3.14159 * self.r * self.r }
    // name() uses default — not overridden
}

impl Shape for Rect {
    fn area(let self) -> Float { self.w * self.h }
    fn name(let self) -> String { "rectangle" }   // override default
}

// Usage — static dispatch (monomorphized)
fn print_area<T: Shape>(let shape: T) {
    print(shape.area())
}

// Usage — dynamic dispatch (opt-in)
fn print_area_dyn(let shape: dyn Shape) {
    print(shape.area())
}
```

### 1.2 Core traits (provided by the standard library)

```axiom
trait Deinit {
    fn drop(inout self)               // destructor — every type auto-implements
}

trait Equatable {
    fn eq(let self, let other: Self) -> Bool
}

trait Hashable: Equatable {           // Hashable requires Equatable
    fn hash(let self) -> Int
}

trait Ord: Equatable {                // Ord requires Equatable
    fn cmp(let self, let other: Self) -> Ordering
}

trait Display {
    fn fmt(let self) -> String
}

trait Clone {
    fn clone(let self) -> Self
}

trait Default {
    fn default() -> Self
}
```

### 1.3 What v1 does NOT include

| Feature | Status | Why |
|---------|--------|-----|
| Associated types | `[Deferred → v2]` | `trait Iterator { type Item; }` — adds complexity |
| Generic traits (`trait Foo<T>`) | `[Deferred → v2]` | Not needed for v1 collections |
| Trait objects (`dyn Trait`) | `[Deferred → v1.1]` | Requires vtable generation; static dispatch covers v1 |
| Supertraits beyond simple bounds | `[Deferred → v2]` | `trait A: B + C` is in v1; complex hierarchies deferred |
| Operator overloading via traits | `[Deferred → v1.1]` | `Add`, `Sub`, `Mul` traits — deferred to keep v1 scope small |
| Blanket impls (`impl<T: A> B for T`) | `[Deferred → v2]` | Powerful but complex; not needed for v1 core |

---

## 2. Type system changes

### 2.1 New type in `Ty`: `TraitRef`

```rust
/// A reference to a trait, used in bounds and impl blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitRef {
    pub name: String,
    pub def_id: DefId,
}
```

### 2.2 Trait definition in the HIR

```rust
/// A trait declaration: `trait Shape { fn area(...) -> ...; }`
#[derive(Debug, Clone)]
pub struct HirTrait {
    pub id: HirId,
    pub name: String,
    pub type_params: Vec<HirTypeParam>,  // generic traits deferred, but field exists for future
    pub methods: Vec<HirTraitMethod>,
    pub span: Span,
}

/// A method declared in a trait.
#[derive(Debug, Clone)]
pub struct HirTraitMethod {
    pub id: HirId,
    pub name: String,
    pub params: Vec<HirParam>,
    pub return_type: Option<HirType>,
    pub body: Option<HirBlock>,  // None = required; Some = default implementation
    pub span: Span,
}
```

### 2.3 Impl block in the HIR

```rust
/// An impl block: `impl Shape for Circle { ... }`
#[derive(Debug, Clone)]
pub struct HirImpl {
    pub id: HirId,
    pub trait_ref: TraitRef,           // which trait
    pub type_def_id: DefId,            // which type
    pub type_name: String,             // for display
    pub methods: Vec<HirImplMethod>,
    pub span: Span,
}
```

### 2.4 Trait bounds (shared with generics)

Trait bounds are already defined in the generics design:

```rust
pub struct TraitBound {
    pub trait_name: String,
    pub trait_def_id: DefId,
}
```

### 2.5 Changes to `FnTy` for method calls

When a trait method is called, the type checker needs to know which trait the method
belongs to. This is handled during name resolution — the THIR stores the resolved
`DefId` of the method, which points back to the trait.

---

## 3. Parser changes

### 3.1 Trait declaration syntax

```ebnf
TraitDecl   = "trait" IDENT "{" TraitMethod* "}" ;
TraitMethod = "fn" IDENT "(" ParamList ")" ("->" Type)? (Block | ";") ;
```

A method with a body (`Block`) is a default implementation. A method without a body (`;`)
is required — implementors must provide it.

```axiom
trait Shape {
    fn area(let self) -> Float;           // required — no body
    fn name(let self) -> String { "shape" }  // default — has body
}
```

### 3.2 Impl block syntax

```ebnf
ImplDecl = "impl" PathExpr "for" PathExpr "{" ImplMethod* "}" ;
ImplMethod = "fn" IDENT "(" ParamList ")" ("->" Type)? Block ;
```

```axiom
impl Shape for Circle {
    fn area(let self) -> Float { 3.14159 * self.r * self.r }
}
```

### 3.3 Trait bound syntax (in generics)

```ebnf
TraitBound  = PathExpr ;
TypeParam   = IDENT (":" TraitBound ("+" TraitBound)*)? ;
```

```axiom
fn max<T: Ord>(let a: T, let b: T) -> T { ... }
struct List<T: Deinit> { ... }
```

### 3.4 Grammar additions

```ebnf
Item        = FnDecl | StructDecl | EnumDecl | TraitDecl | ImplDecl | ... ;
TraitDecl   = "trait" IDENT "{" (TraitMethod)* "}" ;
TraitMethod = "fn" IDENT "(" ParamList ")" ("->" Type)? (Block | ";") ;
ImplDecl    = "impl" TypePath "for" TypePath "{" (FnDecl)* "}" ;
```

---

## 4. Name resolution changes

### 4.1 Trait scoping

Traits are named items in the module scope, like structs and enums:

```axiom
mod shapes {
    trait Shape { ... }       // DefId for the trait
    struct Circle { ... }     // DefId for the struct
    impl Shape for Circle { ... }  // impl block — not a named item, but registered
}
```

### 4.2 Impl registration

When the resolver encounters `impl Shape for Circle`, it:
1. Resolves `Shape` to a `DefId` — verifies it's a trait.
2. Resolves `Circle` to a `DefId` — verifies it's a struct or enum.
3. Registers the impl in the **impl table**: `(trait_def_id, type_def_id) → impl_block`.

The impl table is used during type checking to find method implementations.

### 4.3 Method resolution

When the type checker sees `shape.area()`:
1. Infer the type of `shape` — say `Circle`.
2. Look up all traits that `Circle` implements (from the impl table).
3. For each trait, check if it has a method named `area`.
4. If exactly one match: resolve to that method.
5. If multiple matches: ambiguity error (must disambiguate with `Shape::area(shape)`).
6. If no match: error — no method `area` on type `Circle`.

### 4.4 Orphan rule

An impl block is allowed only if:
- The trait is defined in the current module/package, OR
- The type is defined in the current module/package.

This prevents two packages from implementing the same trait for the same type, which
would create ambiguity.

```axiom
// In package A:
trait Shape { ... }

// In package B (imports Shape from A):
impl Shape for Circle { ... }  // OK — Circle is defined in B

// In package C (imports Circle from B):
impl Shape for Circle { ... }  // ERROR — neither Shape nor Circle defined in C
```

---

## 5. Type checker changes

### 5.1 Trait method type checking

When type-checking `impl Shape for Circle`:
1. For each method in the trait, find the corresponding method in the impl block.
2. Check that the impl method's signature matches the trait method's signature
   (parameter types, return type, conventions).
3. If a trait method has a default and the impl doesn't override it: use the default.
4. If a trait method is required and the impl doesn't provide it: error.

### 5.2 Bound checking

When type-checking a generic function with trait bounds:

```axiom
fn max<T: Ord>(let a: T, let b: T) -> T { ... }
```

1. After all type parameters are resolved to concrete types, check bounds.
2. For `T: Ord`, look up `(Ord, T's concrete type)` in the impl table.
3. If found: bound is satisfied.
4. If not found: error — the type doesn't implement the required trait.

### 5.3 Method dispatch

When type-checking `shape.area()` where `shape: T` and `T: Shape`:

1. Look up `T` in the type parameter scope — it has bound `Shape`.
2. Look up `Shape`'s method `area` — verify it exists.
3. Verify the call arguments match `area`'s parameter types (substituting `T` for `Self`).
4. The return type is `area`'s return type (substituting `T` for `Self`).

### 5.4 Self type in trait methods

Inside a trait method, `Self` refers to the implementing type:

```axiom
trait Clone {
    fn clone(let self) -> Self;   // Self = the type implementing Clone
}

impl Clone for Circle {
    fn clone(let self) -> Self {   // Self = Circle
        Circle { r: self.r }
    }
}
```

### 5.5 Default method inheritance

Default methods are inherited by implementors that don't override them:

```axiom
trait Shape {
    fn area(let self) -> Float;
    fn name(let self) -> String { "shape" }   // default
}

impl Shape for Circle {
    fn area(let self) -> Float { ... }
    // name() inherited from default — Circle.name() returns "shape"
}
```

The type checker verifies that `Circle` has an implementation for every trait method —
either provided in the impl block or inherited from the default.

---

## 6. Monomorphization impact

### 6.1 Trait methods are monomorphized

When `print_area<Circle>(circle)` is called, the monomorphizer:
1. Generates `print_area__Circle`.
2. Inside it, `shape.area()` resolves to `Circle::area`.
3. The call is a direct call to `Circle::area` — no vtable, no indirection.

### 6.2 Built-in trait implementations

The compiler provides automatic implementations for certain traits:

| Trait | Auto-impl |
|-------|-----------|
| `Deinit` | Every type — compiler generates the drop code |
| `Equatable` | Primitives (`Int`, `Float`, `Bool`, `String`) — compiler generates `eq` |
| `Hashable` | Primitives — compiler generates `hash` |
| `Ord` | Primitives — compiler generates `cmp` |
| `Clone` | Primitives and structs with all-Clone fields — compiler generates `clone` |
| `Display` | Primitives — compiler generates `fmt` |

User-defined types can override these auto-impls by providing their own `impl` block.

### 6.3 The `dyn Trait` escape hatch (deferred → v1.1)

Dynamic dispatch via `dyn Trait` is deferred. When implemented:
- `dyn Trait` is a fat pointer: `(data_ptr, vtable_ptr)`.
- The vtable contains function pointers for each trait method.
- Calling a method through `dyn Trait` goes through the vtable (one indirection).
- This is opt-in because it has a visible cost (indirection + heap allocation for the
  vtable), matching Axiom's "visible cost" philosophy.

---

## 7. THIR dump format changes

### 7.1 Trait declarations

```
Trait(Shape)
  Method(area)
    Param(self) : Self
    Return : Float
  Method(name) : DEFAULT
    Param(self) : Self
    Return : String
```

### 7.2 Impl blocks

```
Impl(Shape for Circle)
  Method(area)
    Param(self) : Circle
    Return : Float
```

### 7.3 Trait method calls

```
MethodCall(area)
  Receiver : Circle
  Method : Shape::area → DefId(...)
  Return : Float
```

---

## 8. Testing spec

### 8.1 The six layers

| Layer | What it is | The hole it closes |
|---|---|---|
| **1. Canonical dump** | THIR dump shows trait decls, impl blocks, method dispatch | "I can't see what trait resolution produced" |
| **2. Golden snapshots** | `.ax` fixtures + checked-in `.thir` goldens with traits | "a change silently broke trait resolution" |
| **3. Coverage invariants** | Every required method is implemented; every bound is checked; every impl satisfies the trait | **"a trait contract violation slipped through"** |
| **4. Diagnostics** | Missing impl, unsatisfied bound, signature mismatch → specific error + span | "a trait error is silently accepted" |
| **5. Fuzz / property** | Random trait programs; assert no panic, all impls complete, all bounds satisfied | "the unimagined case" |
| **6. Unit tests** | Pinpoint checks on method resolution, bound checking, default methods, orphan rule | "the subtle trait bug broad tests gloss over" |

### 8.2 Coverage invariants

#### 8.2.1 `impl_complete(thir) -> Result<(), ImplCoverageError>`

Asserts that every `impl Trait for Type` block provides implementations for all required
(non-default) methods of the trait. If a required method is missing, this invariant fails.

#### 8.2.2 `impl_signatures_match(thir) -> Result<(), SignatureError>`

Asserts that every method in an impl block has the same signature (parameter types,
return type, conventions) as the corresponding trait method. If a signature doesn't match,
this invariant fails.

#### 8.2.3 `bounds_satisfied(thir) -> Result<(), BoundError>`

Asserts that for every type parameter with trait bounds, the concrete type it's resolved
to actually has an impl for those traits. (Shared with generics coverage invariants.)

#### 8.2.4 `orphan_rule_respected(thir) -> Result<(), OrphanError>`

Asserts that every impl block satisfies the orphan rule — either the trait or the type
is defined in the current module/package.

#### 8.2.5 `no_duplicate_impls(thir) -> Result<(), DuplicateImplError>`

Asserts that there is at most one `impl Trait for Type` for any given (trait, type) pair
in the same scope. Duplicate impls are a compile error.

### 8.3 Golden snapshot fixtures

#### Parser fixtures

| Fixture | Tests |
|---|---|
| `trait_decl_minimal.ax` | `trait Shape { fn area(let self) -> Float; }` — required method |
| `trait_decl_with_default.ax` | `trait Shape { fn name(...) -> String { "shape" } }` — default method |
| `trait_decl_multi_methods.ax` | Trait with multiple required + default methods |
| `impl_block.ax` | `impl Shape for Circle { ... }` — basic impl |
| `impl_with_override.ax` | Impl overrides a default method |
| `impl_without_override.ax` | Impl inherits default method |
| `trait_bound_in_fn.ax` | `fn foo<T: Ord>(...)` — bound on generic fn |
| `trait_bound_in_struct.ax` | `struct List<T: Deinit> { ... }` — bound on generic struct |
| `multi_bounds.ax` | `fn foo<T: Hashable + Equatable>(...)` — multiple bounds |

#### Type checker fixtures

| Fixture | Tests |
|---|---|
| `trait_method_call.ax` | `shape.area()` — method dispatch through trait |
| `trait_method_infer_self.ax` | Method call where Self is a type param with trait bound |
| `bound_check_pass.ax` | `T: Ord, T=Int, Int: Ord` — bound satisfied |
| `bound_check_fail.ax` | `T: Ord, T=Foo, Foo: !Ord` — bound unsatisfied |
| `impl_signature_mismatch.ax` | Impl method has wrong signature — error |
| `impl_missing_required_method.ax` | Impl doesn't provide required method — error |
| `impl_default_inherited.ax` | Impl inherits default method — OK |
| `impl_default_overridden.ax` | Impl overrides default method — OK |
| `orphan_rule_violation.ax` | Impl for foreign trait + foreign type — error |
| `duplicate_impl.ax` | Two impls for same trait + type — error |
| `self_type_in_trait.ax` | `fn clone(let self) -> Self` — Self resolves correctly |

#### Diagnostic fixtures

| Fixture | Tests |
|---|---|
| `err_no_method_on_type.ax` | Calling non-existent method — error with span |
| `err_unsatisfied_bound.ax` | Missing trait impl — error with span |
| `err_signature_mismatch.ax` | Wrong param types in impl — error with span |
| `err_missing_required_method.ax` | Incomplete impl — error with span |
| `err_orphan_violation.ax` | Foreign trait + foreign type — error with span |
| `err_duplicate_impl.ax` | Duplicate impl — error with span |

### 8.4 Fuzz / property tests

#### 8.4.1 `fuzz_trait_programs(seed) -> FuzzResult`

Generate random programs with trait declarations, impl blocks, and generic functions with
trait bounds. Assert:
- Parser never panics.
- Type checker resolves all trait methods or emits diagnostics.
- Every impl block is complete (all required methods provided).
- Every impl block's signatures match the trait declaration.
- Every bound is checked against the impl table.
- Orphan rule is respected.

#### 8.4.2 `fuzz_method_resolution(seed) -> FuzzResult`

Generate random method calls on types that implement multiple traits. Assert:
- Method resolution terminates.
- Unambiguous methods resolve correctly.
- Ambiguous methods produce clear diagnostics.

#### 8.4.3 `fuzz_default_methods(seed) -> FuzzResult`

Generate random traits with default methods and impls that may or may not override them.
Assert:
- Default methods are inherited correctly.
- Overridden methods are used instead of defaults.
- Every type has an implementation for every trait method (default or provided).

### 8.5 Unit tests

#### Parser

| Test | What it verifies |
|---|---|
| `test_parse_trait_decl_required` | `trait Foo { fn bar(); }` — required method, no body |
| `test_parse_trait_decl_default` | `trait Foo { fn bar() { ... } }` — default method, has body |
| `test_parse_trait_decl_mixed` | Trait with both required and default methods |
| `test_parse_impl_block` | `impl Foo for Bar { ... }` — basic impl |
| `test_parse_trait_bound` | `fn foo<T: Ord>()` — bound parsing |
| `test_parse_multi_bounds` | `fn foo<T: A + B>()` — multiple bounds |

> ✅ Parser tests covered by existing grammar — `trait_def`, `impl_block` in grammar/item.rs.

#### HIR lowering + name resolution

| Test | What it verifies | Status |
|---|---|---|
| `test_trait_decl_required_method` | `trait Shape { fn area(let self) -> Float; }` — required method, no body | ✅ |
| `test_trait_decl_default_method` | `trait Shape { fn name(let self) -> String { "shape" } }` — default method, has body | ✅ |
| `test_trait_decl_mixed_methods` | Trait with both required and default methods | ✅ |
| `test_trait_with_type_params` | `trait Container<T> { fn get(let self) -> T; }` — type param resolves in method return | ✅ |
| `test_impl_block_basic` | `impl Shape for Circle { ... }` — trait + type names resolve | ✅ |
| `test_impl_block_without_trait` | `impl Circle { ... }` — inherent impl (no trait) | ✅ |
| `test_trait_serialize` | TraitDef appears in HIR dump with methods | ✅ |
| `test_impl_serialize` | ImplDef appears in HIR dump with trait→id for Type→id | ✅ |
| `test_no_traits_backward_compatible` | Non-trait code unaffected | ✅ |

#### Name resolution

| Test | What it verifies |
|---|---|
| `test_resolve_trait_name` | `Shape` resolves to trait DefId |
| `test_resolve_impl_trait_and_type` | Both trait and type resolve in impl block |
| `test_register_impl` | Impl is registered in the impl table |
| `test_orphan_rule_local_trait` | Local trait + foreign type → OK |
| `test_orphan_rule_local_type` | Foreign trait + local type → OK |
| `test_orphan_rule_both_foreign` | Foreign trait + foreign type → error |
| `test_method_resolution_direct` | `circle.area()` where Circle has `area` → resolves |
| `test_method_resolution_trait` | `shape.area()` where shape: T: Shape → resolves via trait |
| `test_method_resolution_ambiguous` | Two traits with same method → error |

#### Type checker — impl checking

| Test | What it verifies |
|---|---|
| `test_impl_complete` | All required methods provided → OK |
| `test_impl_incomplete` | Missing required method → error |
| `test_impl_signature_match` | Method signature matches trait → OK |
| `test_impl_signature_mismatch` | Wrong param types → error |
| `test_impl_default_inherited` | Default method inherited → OK |
| `test_impl_default_overridden` | Default method overridden → OK |

#### Type checker — bound checking

| Test | What it verifies |
|---|---|
| `test_bound_satisfied` | `T: Ord, T=Int, impl Ord for Int exists` → OK |
| `test_bound_unsatisfied` | `T: Ord, T=Foo, no impl Ord for Foo` → error |
| `test_multiple_bounds_pass` | `T: A + B, both impls exist` → OK |
| `test_multiple_bounds_one_fail` | `T: A + B, only A exists` → error |
| `test_bound_on_struct_field` | `struct List<T: Deinit>` — bound checked at instantiation |

#### Type checker — method dispatch

| Test | What it verifies |
|---|---|
| `test_dispatch_static` | `print_area<Circle>(circle)` — direct call, no vtable |
| `test_dispatch_through_type_param` | `print_area<T: Shape>(shape)` — call via trait |
| `test_self_type_resolution` | `fn clone(let self) -> Self` — Self = implementing type |

#### Monomorphization

| Test | What it verifies |
|---|---|
| `test_mono_trait_method_call` | `print_area<Circle>` → `Circle::area` is called directly |
| `test_mono_trait_method_dedup` | Two calls to `print_area<Circle>` → one instance |
| `test_mono_default_method` | Default method is monomorphized for the implementing type |

---

## 9. Implementation order

1. ✅ **Parser:** trait declarations, impl blocks, trait bounds. *(Already existed — `trait_def`, `impl_block` in grammar/item.rs, `TraitDef`, `ImplBlock` AST views, `TraitItemList`, `AssocItemList`.)*
2. ✅ **HIR:** `TraitDef`, `ImplDef`, `TraitMethod`. *(Added to `hir/items.rs`; `Item::TraitDef`, `Item::ImplDef` variants; `DefKind::Trait`.)*
3. ✅ **Name resolution:** trait scoping, impl registration, method resolution. *(Traits registered in top-level scope; impl trait/type names resolved; method signatures + bodies resolved with param scope. `self` receiver lowered from `SelfParam`.)*
4. ✅ **Done** — commit `ff55d02`. Trait registry + impl table populated in collect pass; `infer_method_call` resolves methods via impl table (inherent before trait); `Self` resolves to implementing type in impl method bodies; completeness check emits `MissingTraitMethod`; 12 integration tests. Bound checking deferred to generics+traits phase 3.
5. **Monomorphization:** trait method calls become direct calls.
6. **Built-in traits:** `Deinit` (auto-impl for all types), `Equatable`/`Hashable`/`Ord`
   (auto-impl for primitives).
7. **THIR dump:** show trait decls, impl blocks, method dispatch.
8. **Tests:** golden snapshots, coverage invariants, fuzz, unit tests.

Steps 1-4 are the "trait type checking" milestone. Step 5 integrates with the generics
monomorphizer. Step 6 enables the collection type design.

---

## 10. Dependency graph

```
Parser (trait decl, impl, bounds) ──┐
                                    ├──→ Name Resolution (trait scoping, impl registration)
HIR (HirTrait, HirImpl) ───────────┘           │
                                                ├──→ Type Checker (impl checking, bound checking, method dispatch)
                                                │           │
                                                │           ├──→ Monomorphization (trait methods → direct calls)
                                                │           │
                                                │           ├──→ Built-in traits (Deinit, Equatable, Hashable, Ord)
                                                │           │           │
                                                │           │           └──→ Collection types (List<T: Deinit>, Map<K: Hashable + Equatable>)
                                                │           │
                                                │           └──→ THIR dump (show traits, impls, dispatch)
                                                │
                                                └──→ Tests (all layers)
```

**Critical path:** Parser → HIR → Name Resolution → Type Checker → Built-in traits → Collection types.

---

## 11. Implementation with generics

Traits and generics are tightly coupled — trait bounds require generics, and generic
functions with bounds require traits. The recommended implementation order is:

1. **Generics phase 1:** Parser + HIR for type params and type args (no bounds yet).
2. **Traits phase 1:** Parser + HIR for trait decls and impl blocks (no bounds yet).
3. **Generics phase 2:** Type checker — type param scoping, unification.
4. **Traits phase 2:** Type checker — impl checking, method resolution.
5. **Generics + Traits phase 3:** Bound checking — requires both generics and traits.
6. **Monomorphization:** Requires both generics and traits.
7. **Built-in traits:** Requires monomorphization.
8. **Collection types:** Requires built-in traits.

Phases 1-2 can be done in parallel. Phase 3 is the integration point.

---

## 12. Honest open questions

| # | Question | Status |
|---|----------|--------|
| 1 | **`dyn Trait` — when to add?** | **Deferred → v1.1** — static dispatch covers v1 needs |
| 2 | **Operator overloading traits (`Add`, `Sub`, `Mul`)** | **Deferred → v1.1** — keep v1 scope small |
| 3 | **Associated types (`trait Iterator { type Item; }`)** | **Deferred → v2** — adds complexity |
| 4 | **Blanket impls (`impl<T: A> B for T`)** | **Deferred → v2** — powerful but complex |
| 5 | **Auto-impls for compound types** | **Open** — should `Equatable` auto-impl for `struct Foo { x: Int, y: Int }`? |
| 6 | **Trait inheritance depth** | **Open** — is `trait A: B { } trait B: C { }` allowed? How deep? |
| 7 | **Where clause syntax** | **Decided: inline bounds only** — singular idiom |
| 8 | **Impl in different module than type** | **Open** — orphan rule allows it; but how does it interact with visibility? |
