# Borrow checker (Rust) vs Axiom — side-by-side

This shows common Rust borrow-checker scenarios and their Axiom equivalents
under the MVS model (calling-convention borrows + exclusivity rule).

## Scenario 1: Immutable borrow

| | Rust | Axiom |
|---|---|---|
| Code | `fn read(x: &Point) -> i32 { x.x }` | `fn read(let x: Point) -> Int { x.x }` |
| Calling | `read(&p)` — explicit `&` | `read(p)` — `let` is default, unmarked |
| Duration | tracked via lifetime elision | call duration only, can't escape |

## Scenario 2: Mutable borrow

| | Rust | Axiom |
|---|---|---|
| Code | `fn inc(x: &mut Point) { x.x += 1 }` | `fn inc(inout x: Point) { x.x += 1 }` |
| Calling | `inc(&mut p)` | `inc(inout p)` — mutation visible at call site |
| The value | reference type, tracked | calling convention, can't be stored/returned |

## Scenario 3: Two `&mut` to same value — error

```rust
// Rust — compile error
let mut x = 5;
let a = &mut x;
let b = &mut x;   // error[E0499]: cannot borrow `x` as mutable more than once
*a += 1;
```

```axiom
// Axiom — also a compile error
fn mutate(inout a: Int, inout b: Int) { a += 1; b += 1 }
var x = 5
mutate(inout x, inout x)   // error: overlapping inout borrows of `x`
```

Same protection, but Axiom's error is about the **call site** ("overlapping inout
borrows"). No lifetime graph to untangle.

## Scenario 4: Mutable + immutable overlap — error

```rust
// Rust — compile error
let mut v = vec![1, 2, 3];
let first = &v[0];          // immutable borrow starts
v.push(4);                  // error[E0502]: cannot borrow `v` as mutable
println!("{}", first);      // immutable borrow used here
```

```axiom
// Axiom — also a compile error
let first = v[0]            // let borrow of element (value copy or projection)
v.push(4)                   // error: inout borrow of `v` while let borrow is live
print(first)
```

The Axiom compiler's exclusivity pass (§4.3) catches this: an `inout` borrow of `v`
overlaps a still-live `let` borrow. No lifetimes needed — flow analysis over the
scope is enough.

## Scenario 5: Returning a reference

```rust
// Rust — valid, annotated with lifetime
fn get_x<'a>(p: &'a Point) -> &'a i32 { &p.x }
```

```axiom
// Axiom — NOT possible. `let` borrows can't be returned.
// You'd restructure: return the value, or pass a closure to operate in-place.
fn get_x(let p: Point) -> Int { p.x }     // returns a copy, not a reference
```

Because `let`/`inout` are calling conventions and not types, they can never escape
the call. This eliminates the entire class of lifetime annotations — at the cost of
sometimes copying where Rust would return `&T`.

## Scenario 6: Storing a reference in a struct

```rust
// Rust — valid, struct has a lifetime parameter
struct Config<'a> { name: &'a str }
```

```axiom
// Axiom — NOT possible. Structs own their data.
// Use an owned String (heap via Perceus) or restructure so the struct
// outlives the view into it.
struct Config { name: String }   // owns the string
```

## Scenario 7: Iterator invalidation

```rust
// Rust — compile error
let mut v = vec![1, 2, 3];
for x in &v {        // iterator borrows v immutably
    v.push(*x + 1);  // error[E0502]: cannot borrow `v` as mutable
}
```

```axiom
// Axiom — compile error
//
// If Axiom uses external iteration, the loop variable is a `let` borrow
// on the collection, so `v.push(...)` (which needs `inout v`) is rejected
// while the iterator is live.
//
// If Axiom uses internal iteration (like Ruby's .each), no conflict:
//   v.each(fn(let item: Int) { v.push(item + 1) })  — still rejected
//   because the closure captures `let v`, and `push` needs `inout`.
//
// Either way: same protection, no lifetimes.

// Workaround: collect mutations, apply after the loop.
var pending: List<Int> = []
for x in v {
    pending.push(x + 1)
}
for p in pending {
    v.push(inout p)
}
```

## Scenario 8: Borrowing disjoint fields

```rust
// Rust — compiles fine (borrowck is field-sensitive)
struct Point { x: i32, y: i32 }
fn swap(p: &mut Point) {
    let a = &mut p.x;
    let b = &mut p.y;     // OK: different fields
    std::mem::swap(a, b);
}
```

```axiom
// Axiom — TBD (unresolved open question)
//
// The spec's exclusivity rule (§4.3) operates on whole values.
// Whether a field-sensitive analysis (like Rust's) is feasible
// without reference types is an open design question.
//
// Workaround: copy out, swap, write back.
fn swap(inout p: Point) {
    let tmp = p.x
    p.x = p.y
    p.y = tmp
}
```

## Scenario 9: Closure capturing a borrow

```rust
// Rust — closure captures &v
let v = vec![1, 2, 3];
let c = || { println!("{:?}", v); };     // captures &v
// v.push(4);                             // error while c holds &v
c();
```

```axiom
// Axiom — OPEN QUESTION (§8.2 in spec)
//
// How closures capture their environment under MVS is near-foundational
// and not yet decided. Likely options:
//   (a) closures capture by `let` (copy or shared borrow via Perceus)
//   (b) explicit capture list: `fn [let v]() { ... }`
//   (c) closure type parameterized by capture convention
//
// This is one of the two Spike 0 exit-gate questions.
```

## Scenario 10: `match` bindings and exclusivity

```rust
// Rust — partial borrows in match, sometimes ergonomic pain
match &mut opt {
    Some(ref mut x) if *x > 0 => { *x += 1 }   // borrows x
    None => {}                                  // can't access opt elsewhere
}
```

```axiom
// Axiom — match arms are scoped. Each arm gets its own borrow window.
// The spec §7.2 notes this interaction is not yet confirmed in real
// implementation but expected to be more ergonomic than Rust because
// borrows are per-arm, not per-match-expression.
match opt {
    .some(var x) => { x += 1 }     // borrows x for this arm only
    .none => {}
}
```

## Summary

| Concern | Rust mechanism | Axiom mechanism |
|---|---|---|
| Immutable access | `&T` (reference type) | `let` (calling convention) |
| Mutable access | `&mut T` (reference type) | `inout` (calling convention) |
| No aliased mutation | borrow checker (lifetime graph) | exclusivity pass (flow analysis, no lifetimes) |
| Reference can be returned | yes (`&'a T`) | no — `inout`/`let` can't escape |
| Reference stored in struct | yes (struct gets `<'a>`) | no — structs own their data |
| Copy cost for reads | none (pointer) | none for primitives; small for structs (value semantics) |
| Copy cost for writes | none (pointer) | none (inout is direct pointer at runtime) |
| Annotations | `&`, `&mut`, `<'a>`, `'static` | `let`, `inout`, `sink` |
| Learning curve | steep (lifetimes) | moderate (exclusivity rule) |
