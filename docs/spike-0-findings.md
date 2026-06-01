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

15 scenarios, each labeled with *what a programmer would want* (accept/reject), then compared against the checker.

## Result

**15/15 scenarios matched programmer intent. Zero surprises. Zero false-positive friction** (the checker never rejected something the model says should be accepted).

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

## The key insight (why this is so much lighter than Rust)

**Because a borrow is never a value, it can't be stored, and arguments are evaluated eagerly, almost every borrow lives for the span of a single call and is gone before anything else runs.** Rust's borrow checker is painful precisely because references *are* values that live across statements (stored in locals, returned, held while you do other things) — that's what needs lifetimes. Remove stored references and the entire cross-statement class of conflicts disappears.

Concretely, the spike shows the friction collapses to a tiny surface:
- **Bare convention code is trivially permissive.** Cases A, B, D, G, M, N, O all behave exactly as a programmer expects with no ceremony. Notably **N** (mutate two distinct fields simultaneously) and **O** (`f(inout x, g(x))`) *just work* — both are awkward or require workarounds in Rust.
- **The only friction is two narrow cases**, both with well-known idiomatic answers:
  - **C — aliased variable-index mutation** (`xs[i]`, `xs[j]`): the compiler can't prove `i != j`, so it conservatively rejects. This is the *same* hard case Rust has (`split_at_mut`). Answer: provide a `swap`/`split` primitive on collections.
  - **L — reading a field of a value while mutating the whole value** in one call: reject. Answer: bind the field to a local first (`val px = p.x; translate(inout p, px)`). Mild.
- **Closures behave cleanly** (H/I/J/K): the model correctly catches iterator-invalidation and borrow-escape, and correctly allows the safe cases — *without* any special closure annotations from the programmer. The escaping/non-escaping distinction does the work.

## Verdict: preliminary GREEN for Path A

On the core risk — *does the exclusivity rule reject reasonable code?* — the answer from this model is **no, hardly ever.** The friction is dramatically smaller than Rust's and concentrated in two cases that have standard workarounds. This supports proceeding with **Path A**; Path B (the GC-fallback) is **not** triggered.

## Honest limits of this spike (what is NOT yet proven)

This was a focused model, not the language. Before the verdict is *final*, the real implementation must confirm:

1. **Subscripts as yielding lenses (§4.4)** were modeled only as static places, not as the suspend/lend/resume mechanism. The hard projection ergonomics (`xs[i] += 10`, nested projections) need a real prototype.
2. **Loops mutating collections** — no loop construct was tested. Need to confirm `loop x in xs { ... }` + mutation patterns don't reintroduce friction.
3. **No stored-borrow pressure was tested** because the model forbids it by construction. The real question is whether *real programs* ever *want* to store a borrow — if they do often, that's friction that shows up as "why can't I keep this reference?" Needs real `.ax` code to judge.
4. **Perceus reference counting / reuse (§4.5–4.6) was not implemented at all.** The spike only covered exclusivity + moves. RC insertion correctness and elision rate are a separate (more mechanical, lower-risk) investigation.
5. **Diagnostic quality** — the spike's error messages are terse. Path A's tolerability in practice depends heavily on world-class exclusivity diagnostics (§12.1).

## Recommendations for the real implementation

- **Proceed with Path A.** Keep Path B documented but dormant.
- **Build these primitives early** so the two friction cases have idiomatic answers from day one: a collection `swap`/`split` (for case C), and lean on "bind to a local" guidance for case L (a good diagnostic should *suggest* exactly this).
- **The closure escaping/non-escaping analysis is load-bearing and works** — implement it as a first-class part of the ownership pass, defaulting to non-escaping.
- **Re-spike subscripts and loops** before v1's memory model lands (these are the untested corners). Track as follow-ups in `DESIGN_SPEC.md` §15.
- **Invest in exclusivity diagnostics** as a release-gating feature, per §12.1 — the friction cases must produce a message that names the fix.

## Pointer

Prototype: `~/work/axiom-spike/` (`cargo run`). Disposable reference; do not build the real compiler on it.
