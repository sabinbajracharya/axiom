# Spike 0 — Memory Model Findings (Path A)

> **What this is.** The recorded result of the throwaway memory-model prototype mandated by `DESIGN_SPEC.md` §4.10. The prototype code lives at `~/work/axiom-spike/` (a separate, disposable project — kept as a reference, **not** the basis of the real compiler). This file is the deliverable: what we learned and the Path A vs Path B decision.
>
> **Date:** spike run on Rust 1.96.0. **Status of verdict:** *preliminary GREEN for Path A* — favorable on the core risk, with named follow-ups before it's final.

## The question the spike had to answer

`DESIGN_SPEC.md` commits to **Path A** (no GC, no lifetimes, exclusivity discipline) but admits the exclusivity rule is real cognitive load. The risk: that it fires as often and as confusingly as Rust's borrow checker, making the language painful. Spike 0's job was to find out **how often the exclusivity rule rejects code a programmer would consider reasonable** — and to validate the closure-capture model (the near-foundational open question, §8.2).

## What was built

A ~470-line Rust prototype (no parser; ASTs hand-built) implementing:
- the three conventions `let` / `inout` / `sink`;
- the **exclusivity checker** — conflict detection over the set of places *live simultaneously during one call*;
- **place/projection overlap** reasoning (fields, literal indices, variable indices);
- **closure capture** rules (non-escaping may borrow; escaping must `sink`);
- **move tracking** (use-after-`sink`);
- the `val`/`var` × convention interaction (can't `inout`/`sink` an immutable binding).

23 scenarios (15 core + 8 loops/subscripts), each labeled with *what a programmer would want* (accept/reject), then compared against the checker.

## Result

**23/23 scenarios matched programmer intent. Zero surprises. Zero false-positive friction** (the checker never rejected something the model says should be accepted). 9 of the rejections are good bug-catches (use-after-move, iterator-invalidation, borrow-escape, aliased mutation).

| # | Scenario | Verdict | Note |
|---|----------|---------|------|
| A | `add(x, x)` — two shared reads | ✅ accept | reads don't conflict |
| B | `swap(inout xs[0], inout xs[1])` — distinct literal indices | ✅ accept | proven-distinct elements |
| C | `swap(inout xs[i], inout xs[j])` — variable indices | ⛔ reject | **friction frontier** (can't prove i≠j) |
| D | `f(inout x, x)` — mutate + read same, one call | ⛔ reject | real bug caught |
| E | `archive(sink u); read(u)` | ⛔ reject | use-after-move caught |
| F | `rename(inout a)` on a `val` | ⛔ reject | immutable binding |
| G | `rename(inout b)` on a `var` | ✅ accept | |
| H | `xs.for_each(closure inout-capturing xs)` | ⛔ reject | **iterator invalidation caught** |
| I | `ys.for_each(closure reading xs)` | ✅ accept | different collections |
| J | escaping closure capturing `x` by `inout` | ⛔ reject | **borrow-escape caught** |
| K | escaping closure capturing `u` by `sink` | ✅ accept | ownership moved in |
| L | `translate(inout p, p.x)` — read field while mutating whole | ⛔ reject | **friction frontier** (mild) |
| M | `dist(p.x, p.y)` — two distinct field reads | ✅ accept | |
| N | `f(inout p.x, inout p.y)` — mutate two distinct fields at once | ✅ accept | **better than Rust** (no split-borrow ceremony) |
| O | `f(inout x, g(x))` — nested read during outer mutate | ✅ accept | eager-eval releases the read first |
| P | `loop x in xs { total += x }` — fold into another var | ✅ accept | read-iterate + accumulate elsewhere |
| Q | `loop x in xs { xs.push(x) }` — mutate the iterated collection | ⛔ reject | **iterate-while-mutate caught** |
| R | `loop i in 0..n { xs[i] = f(xs[i]) }` — in-place update by index | ✅ accept | **the critical ergonomic case — works** |
| S | `loop x in xs { ys[k] = x }` — write a different collection | ✅ accept | |
| U | `xs[i] = xs[j]` — write one element from another | ✅ accept | eager RHS released before write projection |
| V | `swap(inout cells[0], inout cells[1])` — **user-defined** subscript | ⛔ reject | whole-receiver borrow (see below) |
| X | `grid[i][j] = 0` — nested projection write | ✅ accept | |
| Y | `loop x in xs { add_into(inout acc, x) }` — fold via call | ✅ accept | |

## The key insight (why this is so much lighter than Rust)

**Because a borrow is never a value, it can't be stored, and arguments are evaluated eagerly, almost every borrow lives for the span of a single call and is gone before anything else runs.** Rust's borrow checker is painful precisely because references *are* values that live across statements (stored in locals, returned, held while you do other things) — that's what needs lifetimes. Remove stored references and the entire cross-statement class of conflicts disappears.

Concretely, the spike shows the friction collapses to a tiny surface:
- **Bare convention code is trivially permissive.** Cases A, B, D, G, M, N, O all behave exactly as a programmer expects with no ceremony. Notably **N** (mutate two distinct fields simultaneously) and **O** (`f(inout x, g(x))`) *just work* — both are awkward or require workarounds in Rust.
- **The only friction is two narrow cases**, both with well-known idiomatic answers:
  - **C — aliased variable-index mutation** (`xs[i]`, `xs[j]`): the compiler can't prove `i != j`, so it conservatively rejects. This is the *same* hard case Rust has (`split_at_mut`). Answer: provide a `swap`/`split` primitive on collections.
  - **L — reading a field of a value while mutating the whole value** in one call: reject. Answer: bind the field to a local first (`val px = p.x; translate(inout p, px)`). Mild.
- **Closures behave cleanly** (H/I/J/K): the model correctly catches iterator-invalidation and borrow-escape, and correctly allows the safe cases — *without* any special closure annotations from the programmer. The escaping/non-escaping distinction does the work.

## Loops + subscripts (the second spike, corners now closed)

The extended prototype added a `loop` construct and subscript projections. Two findings matter:

- **The critical loop ergonomic works (R).** `loop i in 0..n { xs[i] = f(xs[i]) }` — mutating a collection element-by-element by index — is **accepted**. This was the make-or-break: if Path A couldn't do in-place index updates in a loop, it would be unusable. It can. The model: a `loop x in coll` holds a *shared* borrow of `coll` for the whole body (so mutating `coll` inside is rejected — **Q**, iterate-while-mutate caught), but `loop i in 0..n` (a range) borrows nothing, so writing `coll[i]` inside is free.
- **Subscripts have two regimes, and this is a real design decision:**
  - **Builtin collections** (List/array): the compiler knows distinct literal indices are disjoint storage, so `xs[0]` and `xs[1]` don't conflict (**B**) — you can mutate distinct elements simultaneously, and `xs[i] = xs[j]` works because the RHS read is released before the write projection opens (**U**, eager eval).
  - **User-defined subscripts**: the accessor is a method that borrows the *whole receiver*, so **any two simultaneous index projections conflict — even with distinct literals** (**V**: `swap(inout cells[0], inout cells[1])` is rejected). To mutate two elements of a user collection at once you need a dedicated `split`/`swap` primitive (exactly Rust's `split_at_mut` situation), or the collection must expose builtin-style disjoint indexing.
  - Variable indices (`xs[i]`,`xs[j]`) conflict for *both* regimes (**C**) — can't prove `i != j`.
- **Nested projections compose** (`grid[i][j] = 0`, **X**) with no extra friction.

## Verdict: preliminary GREEN for Path A

On the core risk — *does the exclusivity rule reject reasonable code?* — the answer from this model is **no, hardly ever.** The friction is dramatically smaller than Rust's and concentrated in two cases that have standard workarounds. This supports proceeding with **Path A**; Path B (the GC-fallback) is **not** triggered.

## Honest limits of this spike (what is NOT yet proven)

This was a focused model, not the language. The two corners flagged in the first run — **subscripts** and **loops** — have now been prototyped (above) and behave well. What remains untested before the verdict is *final*:

1. ~~Subscripts as lenses~~ — **closed** (this run): builtin vs user-subscript regimes characterized; in-place projection composes.
2. ~~Loops mutating collections~~ — **closed** (this run): index-update works, iterate-while-mutate caught.
3. **No stored-borrow pressure was tested** because the model forbids it by construction. The real question is whether *real programs* ever *want* to store a borrow — if they do often, that's friction that shows up as "why can't I keep this reference?" Needs real `.ax` code to judge. *(Still open — the genuine remaining ergonomic risk.)*
4. **Perceus reference counting / reuse (§4.5–4.6) was not implemented at all.** The spike only covered exclusivity + moves. RC insertion correctness and elision rate are a separate (more mechanical, lower-risk) investigation.
5. **Diagnostic quality** — the spike's error messages are terse. Path A's tolerability in practice depends heavily on world-class exclusivity diagnostics (§12.1).
6. **The subscript projection borrow's exact *duration*** (single operation with eager operands, as modeled here) needs to be the committed rule — it is what makes `xs[i] = xs[j]` and `xs[i] = f(xs[i])` work. Confirm it holds for compound cases.

## Recommendations for the real implementation

- **Proceed with Path A.** Keep Path B documented but dormant.
- **Build these primitives early** so the friction cases have idiomatic answers from day one:
  - a collection `swap`/`split` for aliased/var-index and user-subscript mutation (cases C, V) — the `split_at_mut` analogue;
  - the **range-loop + index** pattern (`loop i in 0..n { xs[i] = ... }`) is the sanctioned in-place mutation idiom (case R) — document it as *the* way;
  - lean on "bind to a local" guidance for case L (a good diagnostic should *suggest* exactly this).
- **Decide the subscript borrow duration rule explicitly:** a projection borrow lasts for *one operation with eagerly-evaluated operands* (what the spike modeled). This is what makes `xs[i] = xs[j]` and `xs[i] = f(xs[i])` work; commit it in §4.4.
- **Special-case builtin collection indexing as disjoint storage** so distinct constant indices don't conflict; user-defined subscripts borrow the whole receiver (case V). Document both in §4.4.
- **The closure escaping/non-escaping analysis is load-bearing and works** — implement it as a first-class part of the ownership pass, defaulting to non-escaping.
- **Invest in exclusivity diagnostics** as a release-gating feature, per §12.1 — the friction cases must produce a message that names the fix.

## Pointer

Prototype: `~/work/axiom-spike/` (`cargo run`). Disposable reference; do not build the real compiler on it.
