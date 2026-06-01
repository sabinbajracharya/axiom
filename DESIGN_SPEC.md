# Axiom — Language Design Specification

> **Working names (revisit at 1.0):** language **Axiom**, file extension **`.ax`**, package manager/build tool **`forge`**, community **Axiomites**.
>
> **Status:** Design draft v0.1. This is the authoritative design document. It is written to be *implementable*, not aspirational. Every load-bearing decision is tagged **[Decided]** (settled, build on it) or **[Deferred]** (intentionally postponed behind a version boundary, with the reason it is safe to postpone). Anything that is genuinely uncertain *and* on the critical path is flagged for a **spike** — a throwaway prototype that retires the risk *before* it becomes foundational. There are deliberately **no silent loose ends.**
>
> **Host implementation language:** Rust. **Native backend:** Cranelift (v1), LLVM-tier optional later. **Second backend:** a register-IR interpreter for WASM/browser, mirroring Oxy's dual-backend design.

---

## Table of Contents

0. [Preamble & Identity](#0-preamble--identity)
1. [Philosophy & Design Principles](#1-philosophy--design-principles)
2. [Lexical Structure](#2-lexical-structure)
3. [Type System](#3-type-system)
4. [Memory Model](#4-memory-model) ← *the heart of the language*
5. [Bindings & Mutability](#5-bindings--mutability)
6. [Error Handling](#6-error-handling)
7. [Control Flow](#7-control-flow)
8. [Functions & Closures](#8-functions--closures)
9. [Concurrency](#9-concurrency)
10. [Modules, Packages & Visibility](#10-modules-packages--visibility)
11. [Standard Library (surface)](#11-standard-library-surface)
12. [Tooling & Compiler Ergonomics](#12-tooling--compiler-ergonomics)
13. [Compiler Architecture](#13-compiler-architecture)
14. [Staged Roadmap](#14-staged-roadmap)
15. [Open Questions](#15-open-questions)
16. [Appendix: Grammar, Examples, Glossary](#16-appendix)

---

## 0. Preamble & Identity

**Axiom in one paragraph.** Axiom is a statically typed, compiled, general-purpose language that delivers deterministic memory safety **without a garbage collector and without lifetime annotations**. It reads like Swift/Kotlin, types like Rust (algebraic data types + exhaustive `match`), handles errors like Zig (error sets + `try`/`catch`/`errdefer`), and runs concurrency like Go (colorless green threads), but with one rule the others abandon: **there is one obvious way to do each thing, and the compiler enforces it.** Memory is managed by a hybrid of *Mutable Value Semantics* (borrowing as a calling convention, not a type) and *Perceus* compile-time reference counting. The result targets the gap between application languages (productive but GC-bound) and systems languages (fast but ceremony-heavy).

**Audience decision [Decided].** Axiom targets **Path A — systems-capable**: zero-cost abstractions, no GC, predictable latency. This is a deliberate, eyes-open choice. The cost is that the memory model imposes an **exclusivity discipline** (see §4) that is real cognitive load — softer than Rust's borrow checker, but not free. We accept this cost because the whole point of the language is to prove that *no-GC + no-lifetimes + safety* can coexist with good ergonomics. If at any point the spike (§4.10) shows the exclusivity rule is intolerable in practice, the fallback is **Path B** (value semantics + Perceus with simpler borrows and an optional GC escape hatch) — documented in §4.11 but **not** the chosen path.

**What Axiom is.** A compiled language for building servers, CLIs, tools, and performance-sensitive application code where predictable latency matters and a GC pause is unacceptable, but where Rust's lifetime tax is unwelcome.

**What Axiom is not.** Not a scripting language. Not a pure-functional language. Not a Rust clone (no `&T`/lifetimes). Not a "Rust without borrow checking" (that combination is incoherent — see §1.4).

**How to read this doc.** §1–§3 establish the surface. §4 is the heart — read it slowly. §5–§9 are the rest of the language. §13–§14 are for implementers. §15 is the honest list of what's still open. If you read only one section, read §4.

---

## 1. Philosophy & Design Principles

### 1.1 The Singular Idiom [Decided]

There should be **one obvious way** to express each operation, and where feasible the compiler *enforces* it rather than leaving it to convention. This is elevated from a cultural guideline (Python's Zen) to an architectural rule:

- One loop construct (§7.1), not three.
- One branching tool for ADTs (`match`, §7.2), not `switch` + `else if` chains.
- One mandatory, unconfigurable formatter (§12.3) — formatting is not a matter of taste.
- New syntactic sugar that overlaps an existing construct is rejected at the design level.

**Rationale.** Code is read far more than written. Every redundant way to express a thing forces the reader to decode the author's stylistic choice before reaching the logic. Removing choices reduces cognitive load and — measurably — makes the language friendlier to AI code generation, which thrives on low syntactic ambiguity.

### 1.2 Local Reasoning Is the North Star [Decided]

A reader should be able to understand a function by reading that function. This principle decides several downstream rules:
- **No exceptions** — control flow does not invisibly unwind (§6).
- **Effects/mutation are visible at the call site** — `inout`/`sink` make mutation and consumption explicit where the call happens (§4).
- **No inheritance** — no hidden state from a superclass three levels up (§3.4).

### 1.3 Pragmatic, Not Purist [Decided]

Axiom borrows from functional programming (immutability by default, errors as values, exhaustive ADTs) but is **not** pure-functional. No monads-to-do-IO, no mandatory category theory. Imperative, step-by-step code is first-class; the functional ideas are adopted only where they reduce defects without raising the conceptual barrier.

### 1.4 The Coherence Constraint (why "Rust minus borrow checker" is rejected) [Decided]

Rust's reference syntax (`&T`, `&mut T`, moves, lifetimes) exists *solely* to serve the borrow checker. Remove the checker and that syntax becomes pure ceremony with no payoff — you keep the tax and lose what it bought, landing on C's "share freely and hope" semantics underneath Rust-looking code. That is strictly worse than either parent.

The only two coherent resolutions are:
1. **Add a GC, drop `&` entirely** (Go's answer) — gives up determinism.
2. **Keep determinism, drop the checker, replace references-as-types with borrowing-as-calling-convention + compile-time refcounting** (Hylo/Koka's answer).

Axiom takes resolution **2**. This is the intellectual core of the whole design and the reason the memory model (§4) is the way it is.

### 1.5 Complexity Budget [Decided]

The language has a finite complexity budget and spends nearly all of it on the **memory model**. Everything else is kept deliberately spartan to afford that. Features that would push total complexity past "Rust-lite" — algebraic effects, generational references, a cycle collector, HKT-level generics — are **deferred or rejected** explicitly (§4.7, §9.5, §15), not because they're bad, but because the budget is already spent.

---

## 2. Lexical Structure

### 2.1 Source files
- UTF-8, extension `.ax`. One file = one module (§10). LF line endings canonical; the formatter enforces.

### 2.2 Comments
- Line: `// ...`
- Doc: `/// ...` (attaches to the following item; consumed by doc tooling)
- Block: `/* ... */` (nestable)

### 2.3 Identifiers
- `[A-Za-z_][A-Za-z0-9_]*`. Types and traits are `UpperCamelCase`; functions, variables, fields, modules are `snake_case`. **The formatter/linter enforces casing** (singular idiom extends to naming).

### 2.4 Keywords (reserved) [Decided — initial set]
```
val var fn struct enum trait impl
let inout sink                 // parameter conventions (§4)
match if else loop break continue return
try catch errdefer error panic
mod use pub
scope spawn                    // structured concurrency (§9)
true false self Self
as in is
```
Notably **absent**: `async`, `await`, `while`, `for`, `do`, `switch`, `class`, `extends`, `interface`, `null`/`nil`, `mut`, `&`, `'a`.

### 2.5 Literals
- Integers: `42`, `0xFF`, `0o77`, `0b1010`, `1_000_000` (underscores allowed).
- Floats: `3.14`, `1e-9`, `6.022e23`.
- Bytes: byte values via `as byte` cast or `b'A'`.
- Strings: `"..."` with escapes; raw strings `r"..."`; interpolation **deferred** (see §15 — decide between `"${expr}"` and a `format` function only; the singular idiom forbids having both).
- Booleans: `true`, `false`.
- Char: deliberately **omitted** in v1 — strings are UTF-8; indexing yields a grapheme/`String`, not a `char`. (Revisit if real need appears.)

### 2.6 Numeric types [Decided]
Three numeric types, no width zoo:
- `Int` — 64-bit signed (the default integer).
- `Float` — 64-bit IEEE-754 (the default real).
- `Byte` — 8-bit unsigned, for binary/protocol work.

**Rationale.** Inherits Oxy's hard-won lesson: the `i8/i16/.../usize` zoo is cognitive overhead that rarely pays off outside binary protocols. `Byte` covers the protocol case. Conversions are explicit via `as` (`x as Float`); no implicit widening. **Rejected:** sized integer families — reintroduce only with a load-bearing reason (e.g., a specific binary format), and document it here if ever accepted.

> **Open (low-risk, §15):** whether `Int` overflow is checked (panic), wrapping, or `Result`-returning. Leaning **checked in debug, wrapping opt-in via methods** (`a.wrapping_add(b)`), but this is a §15 decision, isolated from everything else.

### 2.7 Operators & precedence

From tightest to loosest binding:

| Lvl | Operators | Assoc |
|----|-----------|-------|
| 14 | `.` (field/method), `()` (call), `[]` (subscript), `::` (path) | left |
| 13 | `as` (cast) | left |
| 12 | unary `-` `!` | right |
| 11 | `*` `/` `%` | left |
| 10 | `+` `-` | left |
| 9  | `<<` `>>` | left |
| 8  | `&` (bitand) | left |
| 7  | `^` | left |
| 6  | `\|` (bitor) | left |
| 5  | `==` `!=` `<` `<=` `>` `>=` | left (non-chaining) |
| 4  | `&&` | left |
| 3  | `\|\|` | left |
| 2  | `..` `..=` (ranges) | none |
| 1  | `=` `+=` `-=` `*=` `/=` `%=` (assignment, statements only) | right |

Notes:
- `!` on a **type** is the error-union sugar (§6.2); `!` on a **value** is boolean negation. Disambiguated by position (type vs expression context).
- No `++`/`--` (singular idiom: use `+= 1`).
- Comparison does not chain (`a < b < c` is a type error, not `(a<b)<c`).

---

## 3. Type System

### 3.1 Overview [Decided]
Statically typed, nominal, with **local type inference** (infer inside function bodies; **require annotations on all function signatures, struct fields, and public APIs**). No global Hindley-Milner inference — signatures are documentation and the inference boundary, which keeps error messages local (§1.2).

### 3.2 Primitives & built-ins
`Int`, `Float`, `Byte`, `Bool`, `String`, and the unit type `()` (also written `void` as a return). Collections: `List<T>`, `Map<K,V>`, `Set<T>`, and ordered `OrderedMap<K,V>`/`OrderedSet<T>`. Core enums `Option<T>`, `Result<T,E>` (§6).

### 3.3 Structs (product types) [Decided]
```rust
struct Point {
    x: Float,
    y: Float,
}

impl Point {
    fn origin() -> Point { Point { x: 0.0, y: 0.0 } }   // associated fn
    fn dist(let self, other: Point) -> Float { ... }     // method, borrows self
    fn translate(inout self, dx: Float, dy: Float) { ... } // mutating method
}
```
- Methods declare a receiver convention: `let self` (read), `inout self` (mutate), `sink self` (consume). This is the same convention machinery as parameters (§4) — no special `&self`/`&mut self` forms.
- **No inheritance.** Composition + traits only. Flat, predictable memory layout (cache-friendly).
- Field visibility: private by default, `pub` to export (§10).

### 3.4 Enums (sum types) [Decided]
```rust
enum IpAddr {
    V4(Byte, Byte, Byte, Byte),
    V6(String),
}

enum Tree<T> {
    Leaf,
    Node(Tree<T>, T, Tree<T>),
}
```
Payload-bearing variants. The *only* sanctioned way to model mutually-exclusive states. Eliminates null and class-cast polymorphism.

### 3.5 Traits (the only polymorphism) [Decided]
```rust
trait Shape {
    fn area(let self) -> Float;
    fn name(let self) -> String { "shape" }   // default method
}

impl Shape for Circle {
    fn area(let self) -> Float { 3.14159 * self.r * self.r }
}
```
- Traits define shared **behavior**, never shared **state** (no fields).
- **Static dispatch by default** (monomorphized generics). **Dynamic dispatch** via an explicit `dyn Trait` boxed form — opt-in, because it costs an indirection and a heap allocation (visible cost, singular idiom).
- Coherence: a trait impl is allowed only if you own the trait or the type (orphan rule), to keep impls globally unambiguous.

### 3.6 Generics [Decided — bounded, monomorphized]
```rust
fn max<T: Ord>(let a: T, let b: T) -> T { if a > b { a } else { b } }
struct Pair<A, B> { first: A, second: B }
```
- **Monomorphization** (like Rust/C++ templates) — zero-cost, but code-size cost. Trait bounds (`T: Ord`) constrain and document.
- **Deferred [Deferred → v2]:** associated types, higher-kinded types, const generics, variance subtleties. v1 ships plain parametric generics with trait bounds. These are isolated behind the v2 boundary — nothing in v0/v1 depends on them.

### 3.7 Type inference scope [Decided]
- **Inferred:** local `val`/`var` initializers, closure parameter types when the context fixes them, `match` arm result types.
- **Required:** every `fn` parameter and return type, every struct field, every `trait` method signature. Inference never crosses a function boundary.

---

## 4. Memory Model

> This is the heart of Axiom. It is the one place we spend the complexity budget. Read it carefully.

### 4.0 The promise and the price [Decided]
Axiom is **memory-safe, data-race-free, deterministic, and GC-free**, with **no lifetime annotations**. The price is a single discipline the compiler enforces — the **exclusivity rule** (§4.3). This is the borrow checker's *core idea* with a far smaller surface: no `<'a>`, no reference types, no move-vs-borrow ceremony at every binding. You will still occasionally see "value is exclusively borrowed here" errors. That is the irreducible cost of the promise; there is no version of no-GC + no-lifetimes + safety that avoids it entirely (§1.4).

### 4.1 Value semantics foundation [Decided]
Every variable is the **unique owner** of its value. There are no reference *types* — you cannot declare, store, or return a "reference to T." Aliasing does not exist at the type level. This is *Mutable Value Semantics* (MVS), after Hylo.

Consequences:
- Assignment and passing are conceptually *copies of ownership*; the compiler makes them cheap (move, not deep-copy) when it can prove the source is dead afterward, and inserts a real copy only when both sides stay live.
- No two variables can observe each other's mutations through aliasing, because aliasing isn't expressible.

### 4.2 The three parameter conventions [Decided]
How a value crosses a function boundary — and the *only* forms of borrowing in the language:

| Convention | Access | Lifetime | Caller after call | Rust analogue |
|---|---|---|---|---|
| `let` (default) | read-only | duration of the call | unchanged, still owns | `&T` |
| `inout` | exclusive read-write | duration of the call | owns it, now mutated | `&mut T` |
| `sink` | takes ownership | forever | **invalidated** — use is a compile error | `T` (by move) |

```rust
fn read(let u: User) { print(u.name) }
fn rename(inout u: User, n: String) { u.name = n }
fn archive(sink u: User) { db.store(u) }   // u destroyed inside or moved on

var u = User { ... }
read(u)                // borrow-to-read; default, no keyword at call site
rename(inout u, "Sam") // call site states mutation explicitly
archive(sink u)        // call site states consumption explicitly
// u is now invalid; referencing it is a compile-time error
```

**Why this eliminates lifetimes [Decided].** A borrow (`let`/`inout`) is **never a value** — it cannot be returned, stored in a struct, or captured beyond the call. It exists only for the call's duration. *No escape ⇒ nothing to annotate.* Lifetimes exist in Rust only because `&T` is a first-class value whose lifespan must be tracked; remove first-class references and lifetimes vanish.

**Call-site keywords [Decided].** `inout` and `sink` are written at the call site (`rename(inout u, ...)`, `archive(sink u)`); `let` is the default and unmarked. This makes mutation and consumption **visible where the call happens** (§1.2) — you never have to open the callee to know it mutates or consumes its argument.

### 4.3 The exclusivity rule (the one safety invariant) [Decided]
> While an `inout` borrow of a value is active, **no other access to that value — read or write, by any path — may occur.**

This is the entire safety story. It is checked by a flow-sensitive analysis over the AST/IR (the "exclusivity pass", §13). It guarantees data-race-freedom and rules out iterator-invalidation, aliased-mutation, and use-after-consume bugs *at compile time*, without tracking lifetimes.

Examples the compiler **rejects**:
```rust
swap(inout a, inout a)            // same value borrowed inout twice — overlap
let x = a; modify(inout a)        // 'x' reads 'a' while 'a' is inout-borrowed (if x's borrow is still live)
list.push(inout list[0])          // mutating 'list' while projecting into it
```

### 4.4 Subscripts as in-place lenses [Decided]
The hard problem in MVS is **projection**: safely yielding mutable access to *part* of a value (one field of one element of an array) without creating an escaping reference. Solution, after Hylo/Swift: **subscripts that yield rather than return.**

```rust
impl<T> List<T> {
    subscript(let self, i: Int) -> T {     // read projection
        yield self.buffer[i]
    }
    subscript(inout self, i: Int) -> T {   // mutable projection
        yield inout self.buffer[i]         // suspends, lends element, resumes
    }
}

var xs = [1, 2, 3]
xs[1] += 10        // desugars to: take inout projection of element 1, mutate in place, resume
```
A subscript **suspends**, lends a projection to the caller (`let` or `inout`), and **resumes** when the caller is done — writing nothing back through a pointer because there was never a pointer, only a temporary borrow. This gives O(1) in-place element mutation with **no aliasing and no exposed pointer**.

> **Spike target (§4.10):** subscripts + the exclusivity rule + closures are the three features whose *interaction* is the real risk. The spike must exercise all three together.

### 4.5 Heap allocation & Perceus reference counting [Decided]
Value semantics + scope ownership handles the stack-shaped common case: a value is freed deterministically when its owning binding leaves scope (like a C++ destructor / Rust drop). But recursive/shared/escaping data (trees, graphs, closures that outlive a call) needs heap allocation with shared lifetime. For that, Axiom uses **Perceus** (Koka): *precise compile-time reference counting with elision and reuse.*

- The compiler inserts `incref`/`decref` **at compile time**, at precise program points (not on every access like naive ARC).
- **Elision:** when the compiler proves a single owner, it removes the count operations entirely. (Koka/Lobster-class analyses remove the large majority of RC ops; treat exact percentages as best-case, not guaranteed — see §15.)
- **Non-atomic by default:** counts are non-atomic; values shared across green threads use atomic counts only where the type system shows cross-task sharing (ties into §9).
- A value is freed **the instant** its count hits zero — deterministic, cache-local, no background collector, no pause.

### 4.6 Reuse analysis / FBIP [Decided]
Perceus enables **Functional But In-Place**: if the compiler sees a heap value's refcount is exactly 1 at the moment of a "functional update," it compiles the immutable-looking operation into a **destructive in-place mutation**. You write clean functional code (rebuild a tree node); it executes with the cache behavior of hand-written mutable C — *when* the value is uniquely owned. This is the mechanism behind the "close to C on FBIP-friendly code" claim, with the honesty caveat of §15.

### 4.7 Cycles — the explicit gap [Deferred → post-v1, isolated]
Pure reference counting **cannot reclaim reference cycles** (A→B→A). This is a real limitation, named here loudly rather than hidden.

**v1 decision:** ship **without** a cycle collector. Most idiomatic Axiom (value semantics, trees, DAGs) never forms cycles. Doubly-linked / cyclic graphs in v1 are handled by:
- restructuring to a DAG + indices into an arena/`List` (the idiomatic answer), or
- an explicit `Weak<T>` non-owning handle that breaks the cycle (a checked, non-owning index — *not* a raw reference).

**Post-v1:** add an **opt-in trial-deletion cycle collector** (Nim ORC-style, incremental, no stop-the-world) *only if* real programs demonstrate leaks that `Weak`/arenas can't ergonomically handle. This is safely deferred because nothing in v0/v1 depends on it and the `Weak`/arena escape hatch exists from day one.

**Rejected for v1:** Vale-style **generational references**. They're elegant for arbitrary cyclic graphs, but they're a *second* whole memory mechanism layered on the first, and combining MVS + Perceus + generational refs is exactly the "assumes the pieces compose for free" over-reach we're avoiding (§1.5). One spine, not three.

### 4.8 Allocation rules [Decided]
- **Stack:** values whose size is known and whose lifetime the compiler proves is scope-bounded (the common case). Free on scope exit, zero RC.
- **Heap:** values that escape their defining scope (returned, stored in a longer-lived structure, captured by an escaping closure) or are recursive/dynamically sized. Managed by Perceus.
- The decision is the compiler's (escape analysis), not the programmer's — there is no `box`/`new` keyword in the common path. (A `Box<T>` type exists for explicitly heap-forcing recursive enums, e.g. `Node(Box<Tree<T>>, ...)`, mirroring the one place Rust needs it.)

### 4.9 Destruction & ordering [Decided]
- A type may implement `trait Drop { fn drop(inout self) }` for custom cleanup (close file, free FFI handle).
- Destruction order within a scope is **reverse declaration order** (deterministic). `errdefer` (§6.4) composes with this for error-path cleanup.

### 4.10 The memory-model spike (risk retirement) [Required before v1 foundation]
Before any of §4 is committed to the real compiler, build a **throwaway prototype** (`axiom-spike/`, uncommitted) that implements, on a deliberately tiny AST:
1. `let`/`inout`/`sink` conventions + the exclusivity pass (§4.3),
2. subscript projection (§4.4),
3. **closures that capture borrowed/owned values** (§8.2) — the genuinely hard interaction,
4. a minimal Perceus insertion pass (§4.5).

**Goal:** find out *how often* and *how confusingly* the exclusivity rule fires on realistic small programs. **Exit criterion:** either "this is tolerable, proceed with Path A," or "this is intolerable, fall back to Path B (§4.11)." This converts the project's single biggest paper risk into evidence *before* it becomes foundational. **Closures-capturing-borrows is folded into the spike specifically because it is near-foundational and not allowed to remain an open question (§15 names it).**

### 4.11 Path B (documented fallback, NOT chosen) [Reference only]
If the spike fails: keep value semantics + Perceus, but **drop mandatory exclusivity-checked `inout`/`sink`** in favor of simpler copy-or-share borrows, and add an **optional GC/Arc escape hatch** for the cases that would otherwise need exclusivity. This yields a Swift/Go-feeling language: much gentler, loses some zero-cost purity. Recorded so the fallback is a *plan*, not a panic.

---

## 5. Bindings & Mutability

### 5.1 `val` / `var` [Decided]
Two binding forms, on a **different axis** from §4's conventions:
- `val x = ...` — immutable binding: cannot be reassigned; if it owns a collection, the collection's contents cannot be mutated through it.
- `var x = ...` — mutable binding: may be reassigned and (for collections) mutated in place.

```rust
val a = 5
a = 6          // ERROR: a is immutable

var b = [1, 2]
b.push(3)      // OK
b = [9]        // OK
```

**The two axes, stated plainly:**
- `val`/`var` answers *"can I change this binding here, in this scope?"*
- `let`/`inout`/`sink` answers *"what happens to a value when it crosses a function boundary?"*

They compose: you can pass a `val` to an `inout` parameter? **No** — passing `inout` requires a mutable place, so the argument must be a `var` (or a mutable projection). The compiler enforces this, and it's a natural, learnable rule: *you can only lend mutable access to something you're allowed to mutate.*

### 5.2 Shadowing [Decided]
Shadowing in the **same scope** is **disallowed** (singular idiom — `val x = ...; val x = ...` is an error, prevents the "which x is this" reading cost). Shadowing in a **nested** scope is allowed (it's just a different scope). *(This is a deliberate divergence from Rust, which leans on same-scope shadowing; we judge it a readability cost.)*

### 5.3 Constants [Decided]
`const NAME: T = <compile-time-constant-expr>` for module-level compile-time constants (distinct from `val`, which is a runtime immutable binding).

---

## 6. Error Handling

### 6.1 No exceptions [Decided]
There is no `throw`, no stack unwinding for recoverable errors, no hidden control flow. Errors are **values**. `panic(msg)` exists only for **unrecoverable programmer bugs** (broken invariants) and aborts the green thread / process; it is not an error-handling mechanism.

### 6.2 Foundational types & the error-union sugar [Decided]
```rust
enum Option<T> { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }
```
**Error sets** (Zig-style) — named, coercible-to-superset error enums:
```rust
error FsError { NotFound, AccessDenied }
error NetError { Timeout, Refused }
```
**Error union sugar:** `FsError!String` ≡ `Result<String, FsError>`. The `!` in a *type* position is the error union. Sets coerce into supersets automatically, and merge with `||`:
```rust
fn load() -> (FsError || NetError)!Config { ... }   // union of both error sets
```

### 6.3 `try` propagation [Decided]
`try expr` evaluates `expr`; on `Ok(v)` it yields `v`, on `Err(e)` it **returns `Err(e)` from the current function immediately**. The function's return type must be an error union/`Result`. This replaces Go's `if err != nil` boilerplate with one keyword.

### 6.4 `catch` and `errdefer` [Decided]
```rust
fn read_config(path: String) -> FsError!Config {
    val file = try open(path)        // propagate on error
    errdefer log_failure(path)       // runs ONLY if this fn returns Err after this point
    val text = try file.read_all()
    return parse(text)               // success auto-wrapped in Ok
}

fn boot() {
    val cfg = read_config("/etc/app") catch |e| match e {
        FsError.NotFound    => default_config(),
        FsError.AccessDenied => panic("permission denied"),
    }
    // ...
}
```
- `catch |e| <expr>` — evaluates the fallback expression if the LHS is `Err`, binding the payload as `e`. The fallback must produce a value of the success type (or diverge via `return`/`panic`).
- `errdefer <stmt>` — like a deferred cleanup, but runs **only on the error-return path** from its declaration point onward. Composes with `Drop` ordering (§4.9).

### 6.5 Option ergonomics [Decided]
- `?` postfix on `Option` in an `Option`-returning context propagates `None` (parallel to `try` for `Result`). *(One mechanism each — `try` for `Result`, `?` for `Option` — chosen over having both work on both, to keep the rule crisp. **Open §15:** revisit unifying them.)*
- No implicit truthiness; `Option` is consumed by `match` or combinators (`map`, `unwrap_or`, ...).

---

## 7. Control Flow

### 7.1 The unified `loop` [Decided]
One looping keyword, three forms:
```rust
loop { ... }                 // infinite (break to exit)
loop if cond { ... }         // pre-condition loop (replaces while)
loop x in iterable { ... }   // iterator loop (replaces for-each)
```
`break`/`continue` control flow; **labeled loops** for nesting: `'outer: loop { ... break 'outer ... }`. `break value` from `loop {}` makes it an expression yielding `value`.

> *On the "three forms of one keyword" critique:* this is intentional and we judge it *not* a violation of the singular idiom — there is exactly **one** way to write each *kind* of loop (no `while` vs `loop-if` choice, no C-`for` vs `for-each` choice). The forms are disjoint, not overlapping.

### 7.2 `match` — the sole ADT branching tool [Decided]
```rust
match shape {
    Circle(r)   => 3.14159 * r * r,
    Rect(w, h)  => w * h,
}
```
- **Exhaustiveness enforced.** Missing a variant is a compile error. Add a variant ⇒ every `match` on that type fails to compile until updated. This is the compiler-as-refactoring-tool property.
- **Match ergonomics:** destructuring borrows is automatic — no explicit `ref`/`ref mut`. Whether a binding is a read-projection or `inout`-projection follows the scrutinee's convention and the exclusivity rule. *(This auto-binding interacting with §4.3 is on the spike's radar.)*
- Patterns: literals, variant destructure, struct destructure, bindings, `_` wildcard, guards (`Circle(r) if r > 0.0 =>`), `|` alternatives, range patterns (`1..=9 =>`).
- `match` is an **expression**.

### 7.3 `if` / `else` as expressions [Decided]
```rust
val grade = if score >= 90 { "A" } else if score >= 80 { "B" } else { "C" }
```
- No parentheses required around the condition; braces required (no single-statement bodies — singular idiom, prevents the dangling-else and brace-style debates).
- `if let`-style binding is expressed through `match` (one tool), **not** a separate `if let`.

### 7.4 Blocks are expressions [Decided]
A `{ ... }` block evaluates to its final expression (no trailing `;`). `return` for early exit. This unifies statement/expression position and removes the need for ternaries.

---

## 8. Functions & Closures

### 8.1 Functions [Decided]
```rust
fn add(let a: Int, let b: Int) -> Int { a + b }
fn greet(let name: String) { print("hi " + name) }   // -> () implied
```
- All parameters carry a convention (`let` default). Return types required except when `()`.
- **No overloading** (singular idiom — one name, one function). Use distinct names or generics.
- **No default arguments / no variadics** in v1 (each is "more than one way to call"; revisit only with strong cause). Builders or option-structs cover the need.
- Functions are values: `val f = add` then `f(2, 3)`.

### 8.2 Closures [Decided + spike-gated]
```rust
val double = |x| x * 2                       // type inferred from context
val add = |a: Int, b: Int| -> Int { a + b }  // explicit
xs.map(|x| x + 1)
```
**Capture semantics — the genuinely hard part [spike-gated, §4.10]:**
- A closure captures each free variable by a **convention**, just like a parameter: read-capture (`let`), mutating-capture (`inout`), or move-capture (`sink`).
- A **non-escaping** closure (passed to `map`/`filter`/`for_each` and not stored) may capture by `let`/`inout` borrow — the borrow is valid because the closure cannot outlive the call (same "no escape" logic as §4.2).
- An **escaping** closure (returned, stored, spawned onto another task) **must** capture by `sink`/value (or by a refcounted heap value) — it cannot hold a borrow that would escape its source's scope.
- The compiler distinguishes escaping vs non-escaping by analysis; the **default is non-escaping**, and escaping is forced by context (return position, storage, `spawn`).

> **This is the §15 open question that is *not* allowed to stay open** — it is foundational (closures underpin higher-order functions and concurrency). The spike (§4.10) must prove this capture model is sound and ergonomic *before* §4/§8 are committed. If escaping-closure capture proves too restrictive, that's a Path-B signal.

### 8.3 Method call vs free call [Decided]
- `obj.method(args)` — method/field via `.`
- `Type::assoc_fn(args)` and `module::path::fn(args)` — associated/qualified via `::`
- Mirrors Oxy's `.` vs `::` split (field/method vs path).

---

## 9. Concurrency

### 9.1 Colorless execution [Decided]
**No `async`/`await`, no `Future<T>` in signatures.** All functions look and compose identically regardless of whether they block. Concurrency is provided by **green threads** (user-space, M:N scheduled) managed by a runtime scheduler — the Go/Loom/Lua model. A blocking I/O call **parks** the green thread (saving its stack) and yields the OS thread to other runnable green threads; the scheduler resumes it when I/O completes.

**Rationale.** Eliminates function coloring / async-contamination (§ the report's diagnosis is correct). You write linear, synchronous-looking code using ordinary `try`/`catch`/`match` and it runs asynchronously underneath.

### 9.2 Structured concurrency [Decided]
**No `go func()` fire-and-forget.** All concurrency lives in a lexical **scope** (a nursery, à la Trio / Java StructuredTaskScope):
```rust
fn fetch_all(let urls: List<String>) -> List<Response>!NetError {
    scope |s| {
        val handles = urls.map(|u| s.spawn(|| http_get(u)))   // spawn into the scope
        handles.map(|h| try h.join())                         // collect results
    }   // scope cannot exit until every spawned task has finished or been cancelled
}
```
- A `scope` block does not return until **all** tasks spawned in it have completed or been cancelled.
- If a task errors or the scope is cancelled, siblings are **cancelled cooperatively** (at the next park/yield point) — no orphaned tasks, no leaked goroutines.
- Cancellation is structured and hierarchical: cancelling a parent scope cancels its children.

### 9.3 Communication [Decided — channels]
Typed **channels** for message passing between tasks (`Channel<T>`), CSP-style:
```rust
val ch = Channel<Int>::new(capacity: 8)
s.spawn(|| ch.send(compute()))
val v = try ch.recv()
```
Channels are the sanctioned cross-task communication primitive (singular idiom — not channels *and* shared-mutable-locks as co-equal options; locks exist but are positioned as low-level).

### 9.4 Data sharing across tasks under the memory model [Deferred → v2 boundary; isolated]
This is the genuinely subtle interaction: §4's exclusivity is *intra*-task; sharing a value across tasks needs either (a) move it via `sink` into the spawned closure (ownership transfer — the common, safe case), or (b) share an immutable refcounted value (atomic counts, §4.5), or (c) a synchronized cell (`Mutex<T>`) for shared mutation.

**v1 ships only (a) and (b)** — move-into-task and shared-immutable — which cover the vast majority of structured-concurrency patterns and are provably safe under the existing model. **`Mutex<T>`/shared-mutable (c) is deferred to v2.** This is safe to defer because v0/v1 have no concurrency-heavy stdlib depending on shared mutation, and the boundary is explicit. *(This is the §15 open question that legitimately stays open — it sits entirely behind the v2 concurrency boundary and nothing earlier touches it.)*

### 9.5 Algebraic effects — rejected for v1 [Decided / Deferred indefinitely]
The report proposes algebraic effects + handlers as the concurrency backbone. **We reject this for v1** and likely well beyond, for two reasons:
1. **It contradicts "colorless."** Effects tracked in the type signature *are* a coloring (effect rows propagate up the call stack exactly like `async`). You cannot both erase the color (§9.1) and statically track the effect. Pick one; we pick colorless.
2. **Budget (§1.5).** Effects are a multi-year research feature on their own. The green-thread + structured-concurrency model delivers ~90% of the practical value at a fraction of the conceptual cost.

If user-extensible schedulers become a real need later, that's a deliberate post-2.0 research track — not a v1 foundation.

---

## 10. Modules, Packages & Visibility

### 10.1 Modules [Decided]
- One file = one module. Directory structure maps to the module tree.
- `mod name { ... }` for in-file submodules.
- Paths use `::` (`std::io::print`, `mymod::helper`). Field/method access uses `.`. (Same split as Oxy.)

### 10.2 Imports [Decided]
```rust
use std::io::print
use std::collections::{Map, Set}
use mymod::helper as h
```
Glob imports (`use foo::*`) are **discouraged/lint-warned** (singular idiom: explicit names aid the reader). Available but not idiomatic.

### 10.3 Visibility [Decided]
- Everything is **private to its module** by default.
- `pub` exports an item from its module; `pub` is checked at compile time across paths, struct fields, and methods (mirrors Oxy's enforced visibility — private fields/items are genuinely inaccessible from outside, caught at type-check).

### 10.4 Packages — `forge` [Decided; tool deferred to its own milestone]
- A package is a tree of modules with a `forge.toml` manifest + a `forge.lock` lockfile (reproducible builds).
- `forge build`, `forge run`, `forge test`, `forge add <pkg>`, `forge fmt`.
- **Supply-chain:** lockfile is mandatory; dependency build scripts run **sandboxed** (OCI-style isolation); downloaded deps are **cryptographically verified** (Sigstore-style). *(These are design commitments; the implementation lands in the tooling milestone, §14, not v0.)*

---

## 11. Standard Library (surface)

Layered to keep the core small (singular idiom + small budget):

- **`core`** (always available, no_std-able): `Option`, `Result`, primitive methods, `Iterator` trait + adapters (`map`/`filter`/`fold`/`take`/`zip`/...), `String`, `List`, `Map`, `Set`, `OrderedMap`/`OrderedSet`, `Box`, `Weak`.
- **`std`** (hosted): `io` (`print`, `println`, `read_line`, `dbg`), `string` (`format`), `fs`, `env`, `process`, `path`, `time`, `math`, `rand`, `json`, `net`/`http`, `db`. *(Mirrors Oxy's stdlib surface — proven useful, reuse the shape.)*
- **Formatting:** `string::format("{} = {}", k, v)` — **one** formatting mechanism. String interpolation is **not** added unless we drop `format` (singular idiom forbids both; §15).
- **`Display`/`Debug` traits** for user types; `dbg` and `{}`/`{:?}` route through them.

---

## 12. Tooling & Compiler Ergonomics

### 12.1 Paternalistic diagnostics (Elm-grade) [Decided — a hard requirement, not a nicety]
Every error must: echo the offending source lines, point the exact span with carets, explain *why* in plain language, and **suggest a concrete fix** ("you misspelled `List.map` — did you mean `List.map`?"; "`u` was consumed by `archive(sink u)` on line 12; you can't use it on line 14"). The exclusivity errors (§4.3) in particular **must** be world-class — they are the language's hardest concept and a hostile message there sinks adoption. *Diagnostic quality is a release-gating criterion, tracked like a feature.*

### 12.2 LSP & JSON output [Decided]
- Compiler emits structured diagnostics via `--report=json` for zero-friction LSP integration (red squiggles, jump-to-def, rename) — same philosophy as Oxy's LSP-reads-from-`symbols` design.
- Ship an LSP server from early (it's how modern devs experience the language).

### 12.3 The formatter [Decided]
A single, **unconfigurable** formatter (`forge fmt`). Bracket placement, indentation, line length, import ordering — all fixed. The compiler/CI can require formatted code. Formatting debates are eliminated by construction (Go's lesson, kept).

---

## 13. Compiler Architecture

### 13.1 Pipeline [Decided]
```
source
  → lex            (tokens)
  → parse          (AST; Pratt parser, precedence table §2.7)
  → resolve        (name/module/use resolution, visibility)
  → typecheck      (inference within fns, trait resolution, exhaustiveness)
  → ownership      (exclusivity pass §4.3 + escape analysis §4.8 + capture model §8.2)
  → rc-insert      (Perceus incref/decref + reuse analysis §4.5–4.6)
  → ir-gen         (register IR + CFG)
  → codegen        (IR → Cranelift CLIF → native)   |  IR-interp (wasm)
```
- **The `ownership` pass is the new, defining stage.** It is where Axiom differs structurally from Oxy. It runs after typecheck (needs types) and before IR (informs RC insertion and move-vs-copy). Treat it as the highest-risk component (spike it, §4.10).
- **`rc-insert`** is a distinct pass so RC logic is auditable and testable in isolation (Perceus correctness is subtle).

### 13.2 Dual backend [Decided — reuse Oxy's architecture]
Two backends consuming **one register IR**, exactly as Oxy does:
- **Cranelift JIT/AOT** for native (x86/aarch64).
- **Register-IR interpreter** for `wasm32` (browser playground/tutorial).

Both delegate runtime semantics to a **shared FFI layer** so they cannot diverge. Carry over Oxy's three divergence guards: (1) exhaustive `match` over IR ops in the interpreter with **no wildcard arm** (compile-time guard), (2) FFI-surface consistency test between codegen decls and interpreter symbols, (3) a JIT↔interp **parity test** running a corpus through both and diffing. *These guards are among the most valuable things to inherit from Oxy.*

### 13.3 Host language & infra reuse [Decided]
- Written in **Rust** (§ matches the domain — building an ownership language in an ownership language; reuses Cranelift + Oxy's infra).
- **Harvest from Oxy (study + re-implement around new semantics, not `cp -r`):** the pipeline skeleton, dual-backend + FFI + divergence guards, the `symbols.rs` single-source-of-truth pattern, the `.ax` feature-test harness (runtime `#[test]` + `#[compile_error]` tests), IR snapshot tests, the LSP scaffold, and `tug`→`forge`.
- **Do not harvest:** `Value`, the type checker, ir_gen semantics — these are rebuilt because the `Value` representation must now carry RC metadata and the ownership model changes everything downstream.

### 13.4 IR & `Value` representation [Decided — design point]
- Register IR with explicit basic blocks + terminators (Oxy-shaped), extended with **ownership-annotated operands** (each IR value knows whether it's owned/borrowed) and explicit `incref`/`decref`/`reuse` ops so RC is visible in IR and snapshot-testable.
- Heap `Value`s carry a refcount header (and a generation field only if §4.7's collector is ever added — left out in v1).

---

## 14. Staged Roadmap

Each stage is independently shippable and testable. Risk is retired front-to-back.

**Spike 0 — Memory-model prototype (throwaway).** §4.10. Retire the single biggest risk (exclusivity + subscripts + closure capture + minimal Perceus). **Exit gate: Path A confirmed tolerable, or fall back to Path B.** *Nothing permanent is built until this passes.*

**v0 — End-to-end skeleton, NO memory model.** Lex → parse → typecheck → IR → Cranelift, for a value-semantics subset with naive "copy/refcount everything" (no exclusivity, no Perceus optimization). Goal: a `hello.ax` and basic programs run natively. Proves the pipeline. Includes the dual-backend + parity harness early.

**v1 — The memory model.** Fold the spiked ownership pass + Perceus + reuse analysis into the real compiler. Structs, enums, traits, generics (basic), `match` exhaustiveness, `val`/`var`, the three conventions, subscripts, error handling (`try`/`catch`/`errdefer`/error sets). **This is the language's identity landing.** Closures with the spiked capture model. Diagnostics for exclusivity errors held to release-gating quality.

**v2 — Concurrency + ecosystem.** Green-thread scheduler, `scope`/`spawn`, channels, shared-immutable + move-into-task sharing. `forge` package manager with lockfiles + sandboxed builds + signature verification. LSP feature-complete. `Mutex<T>`/shared-mutable cell. Associated types / richer generics.

**v2.x+ — Optional & research.** Opt-in cycle collector (§4.7) *if* leaks prove real. LLVM-tier backend to close the gap to Rust/C (§ perf). String interpolation decision. Self-hosting exploration.

**Cross-cutting, every stage:** per-folder `README.md` docs (Oxy's discipline), the singular-idiom enforcement in the formatter/linter, and the parity test kept green.

---

## 15. Open Questions

Honest list. Each is tagged with whether it may remain open (isolated behind a boundary) or **must be resolved by a spike** before the layer it touches is built.

| # | Question | Status |
|---|----------|--------|
| 1 | **Closure capture of borrowed values** (§8.2) — sound & ergonomic? | **MUST resolve in Spike 0** — near-foundational, not allowed to stay open |
| 2 | Subscript × exclusivity × match-ergonomics interaction (§4.4/§7.2) | **MUST resolve in Spike 0** |
| 3 | How painful does the exclusivity rule feel on real code? | **Spike 0 exit gate** (Path A vs B) |
| 4 | Cross-task **shared-mutable** data (`Mutex<T>`) (§9.4) | **May stay open** — behind v2 boundary, nothing earlier depends on it |
| 5 | Cycle collection (§4.7) | **May stay open** — `Weak`/arena escape hatch exists; add collector only if leaks prove real |
| 6 | `Int` overflow: checked / wrapping / `Result` (§2.6) | May stay open — leaning checked-debug + explicit wrapping methods; isolated |
| 7 | String interpolation vs `format` only (§2.5/§11) | May stay open — singular idiom forbids both; decide before stdlib freeze |
| 8 | Unify `try` (Result) and `?` (Option) into one mechanism? (§6.5) | May stay open — cosmetic; decide before 1.0 |
| 9 | Exact Perceus elision rate on real code (§4.5) | Empirical — measure in v1, don't promise a number |
| 10 | Algebraic effects ever? (§9.5) | Rejected for v1; post-2.0 research track if user-schedulers needed |

**The discipline (restated):** an item may stay open **only** if it's provably isolated behind a version boundary and nothing earlier builds on it. Items 1–3 are on the critical path, so they are **not** documented-and-deferred — they are **spiked to resolution before the foundation is poured.**

---

## 16. Appendix

### 16.1 Grammar sketch (EBNF, partial)
```ebnf
program     = { item } ;
item        = fn_def | struct_def | enum_def | trait_def | impl_block
            | mod_def | use_decl | const_def | error_def ;

fn_def      = "pub"? "fn" ident generics? "(" params? ")" ret_type? block ;
params      = param { "," param } ;
param       = conv? ident ":" type ;
conv        = "let" | "inout" | "sink" ;
ret_type    = "->" type ;

struct_def  = "pub"? "struct" ident generics? "{" { field "," } "}" ;
field       = "pub"? ident ":" type ;
enum_def    = "pub"? "enum" ident generics? "{" { variant "," } "}" ;
variant     = ident ( "(" type { "," type } ")" )? ;

trait_def   = "pub"? "trait" ident "{" { fn_sig | fn_def } "}" ;
impl_block  = "impl" generics? type ( "for" type )? "{" { fn_def | subscript } "}" ;
subscript   = "subscript" "(" conv "self" "," param ")" ret_type block_yield ;

error_def   = "error" ident "{" { ident "," } "}" ;

stmt        = let_stmt | expr_stmt | return_stmt | errdefer_stmt ;
let_stmt    = ("val" | "var") pattern ( ":" type )? "=" expr ;
errdefer_stmt = "errdefer" stmt ;

expr        = literal | path | call | method_call | match_expr | if_expr
            | loop_expr | closure | struct_init | try_expr | catch_expr
            | binary | unary | subscript_access | block ;

match_expr  = "match" expr "{" { arm "," } "}" ;
arm         = pattern ( "if" expr )? "=>" expr ;
loop_expr   = "loop" ( "if" expr | ident "in" expr )? block ;
try_expr    = "try" expr ;
catch_expr  = expr "catch" "|" ident "|" expr ;
closure     = "|" params? "|" ( "->" type )? ( expr | block ) ;

type        = path generics_args? | "(" ")" | error_union ;
error_union = type "!" type ;        (* sugar for Result<type, type> *)
```

### 16.2 Example program — the language end-to-end
```rust
use std::io::println

error ParseError { Empty, NotANumber }

struct Stats { count: Int, sum: Int }

impl Stats {
    fn new() -> Stats { Stats { count: 0, sum: 0 } }
    fn add(inout self, n: Int) { self.count += 1; self.sum += n }
    fn mean(let self) -> Option<Float> {
        match self.count {
            0 => None,
            _ => Some(self.sum as Float / self.count as Float),
        }
    }
}

fn parse_int(let s: String) -> ParseError!Int {
    if s.is_empty() { return Err(ParseError.Empty) }
    s.to_int() catch |_| Err(ParseError.NotANumber)
}

fn main() {
    var stats = Stats::new()
    val inputs = ["10", "20", "oops", "30"]

    loop line in inputs {
        val n = parse_int(line) catch |e| match e {
            ParseError.Empty     => 0,
            ParseError.NotANumber => continue,   // skip bad input
        }
        stats.add(inout stats, n)   // (illustrative: method form is stats.add(n))
    }

    match stats.mean() {
        Some(m) => println(string::format("mean = {}", m)),
        None    => println("no data"),
    }
}
```

### 16.3 Concurrency example
```rust
use std::http
use std::io::println

fn fetch_all(let urls: List<String>) -> List<String>!http::Error {
    scope |s| {
        val handles = urls.map(|u| s.spawn(|| http::get_text(u)))
        var out = []
        loop h in handles { out.push(try h.join()) }
        out
    }
}
```

### 16.4 Glossary
- **MVS** — Mutable Value Semantics: references without reference types; every variable uniquely owns its value.
- **Convention** — how an argument crosses a function boundary: `let`/`inout`/`sink`.
- **Exclusivity rule** — while an `inout` borrow is live, no other access to that value; the sole safety invariant.
- **Perceus** — precise compile-time reference counting with elision + reuse.
- **FBIP** — Functional But In-Place: functional-looking code compiled to in-place mutation when refcount == 1.
- **Subscript** — a yielding accessor that lends an in-place projection (read or `inout`) without an escaping reference.
- **Structured concurrency** — all tasks bound to a lexical `scope`; parent cannot exit until children finish/cancel.
- **Colorless** — functions are identical whether or not they block; no `async`/`await`.
- **Spike** — a throwaway prototype that retires a risk before it becomes foundational.

---

*End of design draft v0.1. The next concrete step is **Spike 0** (§4.10): a throwaway prototype of the memory model whose exit gate decides Path A vs Path B. Nothing permanent should be built before that gate.*
