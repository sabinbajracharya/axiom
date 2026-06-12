# Axiom — Foundation Hardening Plan

> **Purpose & scope.** This plan is **not** about implementing the spec's deferred
> features (iterator loops, closures, the MVS + Perceus memory model, Cranelift,
> concurrency, more stdlib). Those are the *later* work. This plan is strictly
> about **closing the gaps in the code that already exists** so that, when that
> feature work begins, it lands on a solid, consistent, well-tested base instead
> of fighting the foundation.
>
> The litmus test for every item below: *"Does this make the codebase more
> correct, consistent, tested, or architecturally ready — without building a new
> language feature?"* If an item would implement a spec feature, it is **out of
> scope** and lives in the closing section "[What this unblocks](#what-this-foundation-unblocks-deferred-feature-work)"
> — listed only so the boundary is explicit.
>
> **Method.** Every claim was verified against the **code**, not the design docs
> (the docs and the code have drifted — see Phase 1). File:line citations are
> given so each item can be picked up cold.
>
> **The one hard rule (from `DESIGN_SPEC.md` §14):** the memory model (ownership +
> Perceus) is v1's identity and does **not** land before v0 is a defensible,
> tagged baseline. This plan's job is to make that baseline defensible.

---

## Label legend

| Label | Scope |
|---|---|
| `enforcement` | The lint/format/test gate and the conventions it mechanizes |
| `docs-sync` | Reconciling design docs with the actual code |
| `testing` | Test harness, drift guards, coverage invariants, output oracle |
| `diagnostics` | Error-message quality: spans, wording, §12.1 "Elm-grade" bar |
| `arch` | Crate/file structure, size caps, refactors |
| `ir` | The register IR layer (`crates/ir`) |
| `runtime` | The shared runtime/FFI surface both backends must call |
| `backend` | Cranelift native backend + dual-backend parity |
| `memory-model` | MVS conventions, exclusivity checker, Perceus, reuse analysis |
| `control-flow` | Loops / iteration |
| `closures` | First-class function values |
| `error-handling` | `?` / `catch` / `else` / error sets |
| `modules` | `use`, visibility, multi-file graph |
| `stdlib` | The embedded `.ax` standard library |
| `cli` | Driver command surface |

---

## Snapshot — where the code actually is (verified)

**Works end-to-end on the VM** (`cargo run -p cli -- run`): functions/recursion,
`val`/`var`/`let`, arithmetic/comparison/logic, `if`/`match` (exhaustiveness
checked), infinite `loop` and `loop if`, structs, enums (incl. generic enums),
traits with default methods + bounds + orphan/impl-completeness checks, generics
+ monomorphization, `inout`/`sink` conventions (as *calling conventions*, not
enforced), `HeapBuffer<T>` indexing, subscript decls, and an `.ax` stdlib
(`Option`/`List`/`Map`/`print`/`format`). Error handling (`error` sets, `Type!T`,
`?`, `catch`, `else`) type-checks **and runs** (`?` desugars to real `match` post-
typecheck). Verified via `showcase/showcase.ax` and `corpus/valid/error_*.ax`.

**The honest gaps** (each is an item below):
- The enforcement gate is **currently red** (`clippy -D warnings` fails).
- Docs describe a crate layout and milestone map that no longer match the code.
- The corpus is only `check`ed, never **run with output asserted** — no stdout oracle.
- Type-checker diagnostics all report span `0:0` — no source positions.
- `loop x in items` (iterator loops) is **not supported** (the Iterator trait was reverted).
- Closures are **parsed only** — not lowered, typed, or executed.
- There is **no second backend** and **no shared runtime crate**: only the VM exists; `axiom build` is a stub; the "dual-backend parity" architecture is absent.
- The memory model (exclusivity checker, Perceus, reuse) is **absent**; conventions are threaded through HIR but never enforced; the VM clones everything and its heap `refcount` field is dead.

---

## Phase 0 — Stop the bleeding (gate must be green)  ·  ✅ **DONE**

The whole enforcement story (`ENFORCEMENT.md`) is the safety net every later phase
relies on. It was red; it is now green (`fmt` + `clippy -D warnings` + `test` all
pass, plus a CI job that enforces it).

- **F0.1 `enforcement`** ✅ — Fixed the `clippy -D warnings` failure.
  `crates/typecheck/src/typeck/collect.rs` called `.unwrap()` on a user-reachable
  path (`impl_method.unwrap()`), violating `unwrap_used = "deny"`. Restructured to a
  `let Some(impl_method) = … else { … continue }` so the value is bound once. (Also
  fixed pre-existing `cargo fmt` drift in `crates/lower/tests/traits.rs`.)
  *Acceptance met:* `cargo clippy --all-targets -- -D warnings` is clean.
- **F0.2 `enforcement`** ✅ — Added `.github/workflows/gate.yml`, a repo-level CI job
  running the full `fmt --check && clippy -D warnings && test` gate on push/PR, so a
  red gate fails visibly outside the agent's PostToolUse hook.
  *Acceptance met:* a green/red signal now exists in CI.
- **F0.3 `arch`** ✅ — Brought the three over-cap files under the ≤600-line rule:
  `typeck/mod.rs` (649 → 264; check-pass impl extracted to `typeck/check_pass.rs`),
  `typeck/collect.rs` (612 → 309; trait/impl collection extracted to
  `typeck/collect_impls.rs`), and `desugar/.../pre_typecheck/tests.rs` (659 → split
  into a `tests/` dir module: `mod.rs` + `list_and_errors.rs` + `invariants.rs`).
  *Acceptance met:* no `.rs` file under `crates/` exceeds 600 lines.

---

## Phase 1 — Make the map match the territory (`docs-sync`)

Future work is guided by docs; right now they mislead. This is cheap and unblocks
trust + onboarding.

- **F1.1 `docs-sync`** — Reconcile crate names. `docs/v0-roadmap.md` and parts of
  `CLAUDE.md` reference `axiom-lexer / axiom-hir / axiom-typeck / axiom-ir /
  axiom-interp / axiom-codegen / axiom-runtime`. The real crates are
  `lexer / parser / lower / resolver / desugar / typecheck / specialize / ir / vm /
  modules / stdlib / driver / cli`. Update the docs (the `README.md` table is the
  accurate one — align everything to it).
- **F1.2 `docs-sync`** — Rewrite the milestone map. The M1–M6 plan in
  `v0-roadmap.md` (HIR=M1, typeck=M2, IR=M3, interp=M4, Cranelift=M5) describes a
  state already overtaken: HIR/resolve/desugar/typeck/specialize/IR/VM all exist
  and run generics+traits+stdlib. State plainly what is **done** vs **the true
  remaining v0 work** (this document's Phases 2–5).
- **F1.3 `docs-sync`** — Clarify the backend story everywhere (the CLAUDE.md
  "register-IR interpreter for WASM" line): the VM is the **portability + parity-
  oracle** engine; Cranelift AOT is the native backend (not built); `.wasm` emit is
  a separate v2.x+ backend. The README already says this; make CLAUDE.md and the
  roadmap agree.
- **F1.4 `docs-sync`** — Add a `crates/ir/README.md` (the only crate missing its
  per-folder README; `RUST_CONVENTIONS.md` requires one).

---

## Phase 2 — Trustworthy tests (`testing`) — the safety net for everything after

The pipeline runs real programs, but nothing asserts they produce the *right*
output, and the parity discipline that's supposed to keep two backends honest
doesn't exist. Build the oracle now, while the VM is the only backend — so when
Cranelift and the memory-model passes arrive, regressions are caught automatically.

- **F2.1 `testing`** — **End-to-end stdout oracle over the corpus.** Today
  `crates/cli/tests/features.rs` only runs `compile_source` (lex→…→typecheck) and
  asserts clean/error. It never executes programs. Add a golden-output harness:
  each `corpus/valid/**.ax` runs through `ir::lower` + `vm`, and its stdout is
  snapshotted to a checked-in `*.out` (regenerated with `UPDATE_SNAPSHOTS=1`),
  exactly like the per-stage goldens. This is the input→output loop the roadmap's
  "M4 oracle" promised but never landed.
- **F2.2 `testing`** — **Run the corpus *with the stdlib*.** `compile_source`/
  `check_source` funnel through `check_modules(&[("", source)])` — an *empty*
  stdlib (`crates/driver/src/lib.rs:90`). The corpus therefore can't exercise
  `List`/`Map`/`Option`. Add a corpus tier that compiles on the embedded stdlib
  (`stdlib::modules()`, as `compile_dir` in `crates/cli/src/lib.rs:120` does) and
  seed it with collection/Option programs.
- **F2.3 `testing` `backend`** — **Stand up the parity harness skeleton now**, with
  the VM as the sole backend and an `INTERP_ONLY` / `NATIVE_UNSUPPORTED` marker
  mechanism. When Cranelift lands (Phase 5) it plugs into an existing harness
  instead of inventing one. Write `docs/backend-parity-testing.md` first.
- **F2.4 `testing`** — **Resolver coverage/drift guard.** Lexer, parser, lower,
  typecheck, IR, and desugar each have a "can't silently drift" guard
  (`symbol_consistency`, `test_lowerer_handles_every_ast_node_kind`,
  `crates/typecheck/src/coverage.rs`, `crates/ir/tests/desugar_coverage.rs`). The
  **resolver** has none. Add one (every namespace/binding kind is resolved or
  explicitly `NotYetSupported`).
- **F2.5 `testing`** — **VM trap-determinism fixtures.** The VM defines ~17 traps
  (`crates/vm/src/error.rs`). Add fixtures asserting each (div-by-zero, OOB index,
  arity mismatch, match fallthrough, step-limit) behaves deterministically — these
  become regression bedrock for the memory model, which will add ownership traps.

---

## Phase 3 — Diagnostics to the §12.1 bar (`diagnostics`)

`DESIGN_SPEC.md` §12.1 makes "Elm-grade paternalistic diagnostics" a **hard
requirement, not a nicety**. The front-end is total and recovers, but the
type-checker throws away the one thing good messages need.

- **F3.1 `diagnostics`** — **Wire real spans through type-checking.**
  `crates/typecheck/src/typeck/control.rs:171` `span_for` always returns
  `Span { lo: 0, hi: 0 }`, so **every** type error reports `0:0`. Thread the HIR's
  source spans (the lexer/parser already produce them) through lowering into THIR
  so `span_for(id)` returns the true span. Without this, no type diagnostic can
  meet §12.1 and every future feature inherits poor errors. *(Note: the §1.1 quote
  in the spec is illustrative — the real test is the corpus `*.stderr` fixtures.)*
- **F3.2 `diagnostics`** — **Diagnostic-fixture coverage for type errors.** Once
  spans are real, add/strengthen `*.ax` → `*.stderr` fixtures (mismatch,
  non-exhaustive match, unknown field/variant, arity, trait-method mismatch) so
  message quality is locked by snapshot and can't silently regress.

---

## Phase 4 — Make the half-built surface consistent (no silent traps)

Several decided constructs are **partially wired** in the code: parsed but not
lowered, or lowered but rejected later, or lowered into something that would
misbehave. **Implementing them is feature work (out of scope — see the unblocks
section).** The *foundation* gap is that they are inconsistent across stages and
can become silent foot-guns. The work here is to make each one **cleanly and
explicitly rejected at the earliest stage, with a good diagnostic, and tracked** —
so no half-path can ever execute incorrectly, and so the feature can later be
implemented against a clean slate rather than a tangle of partial wiring.

- **F4.1 `control-flow` `arch`** — **Iterator loops `loop x in items` are
  half-lowered.** Typecheck rejects them with `NotYetSupported`
  (`crates/typecheck/src/typeck/control.rs:280`), but IR lowering still emits an
  infinite jump that *ignores the iterable* (`crates/ir/src/lower/expr.rs:485`).
  That dead, wrong lowering is a trap waiting for the day typecheck stops
  rejecting. Gap-closing action (not the feature): make the rejection the single
  source of truth — either lower to an explicit `Unreachable`/trap or remove the
  bogus IR path — and add a fixture pinning the clean diagnostic. Record the
  reverted Iterator-trait history (git `8ee66f7`/`d8315af`/`7342a4a`/`29d7e09`) as
  a tracked design-open item so the next attempt starts informed.
- **F4.2 `closures` `arch`** — **Closures are parsed-only.** The parser produces
  `ClosureExpr`, but HIR has no `Expr::Closure` (`crates/lower/src/hir_types/mod.rs:189`)
  and `lower_expr` has no case, so a closure silently falls through to the generic
  `unsupported_expr` fallback (`crates/lower/src/lowering/expr.rs:76`). Gap-closing
  action: ensure closures hit an **explicit, well-spanned `NotYetSupported`** in
  lowering (not the generic catch-all), and add a fixture. This converts a silent
  trap into a tracked, clearly-diagnosed gap — leaving the actual implementation to
  feature work, which `map`/`filter`/concurrency will need.
- **F4.3 `modules`** — **Glob imports `use foo::*`** already emit `NotYetSupported`
  cleanly (`crates/resolver/src/resolve/mod.rs:251`) — this one is *correctly*
  gated. The only foundation task is a tracking note so it isn't forgotten; no code
  change needed.

> **Audit task F4.0 `arch` `testing`** — Sweep the whole front-end for any *other*
> parsed-but-unlowered or lowered-but-unrejected construct (grep the AST node kinds
> against the lowering/typeck `match`es) and confirm each is caught by an
> *explicit* `NotYetSupported`, never the generic fallback. The drift guards
> (F2.4 + existing) should make every such gap impossible to miss.

---

## Phase 5 — Architectural readiness for the backend & memory-model work

The IR is the single lowering target both future backends consume **and** where
the future ownership + Perceus passes will run. The foundation gap is not "we lack
a second backend" (that's feature work) — it's that **the IR has no clean runtime
boundary, no second consumer validating it, and incomplete invariants**, so any
future pass or backend would build on unproven ground. Close those gaps; the
backend/memory-model *implementations* stay out of scope.

- **F5.1 `runtime` `arch`** — **Extract a shared `runtime` crate.** Today the VM
  implements builtin/extern semantics inline (`crates/vm/src/exec/builtins.rs`,
  whose own comment says the Rust bodies "stand in until real FFI"). Pull the
  language semantics (print/format, value ops, aggregate alloc, the `axiom_*`
  surface) into one safe-Rust crate the VM calls. This is a pure refactor (no new
  behavior) that creates the boundary a future native backend would link against —
  without it, the parity discipline the architecture is built on is structurally
  impossible.
- **F5.2 `ir`** — **Finish the IR's loose ends and harden its invariants.**
  - Error-propagation paths in lowering are marked "not yet wired"
    (`crates/ir/src/lower/expr.rs:50`, `:55`). Now that `?`/`catch`/`else` desugar
    to `match` and run, confirm these stubs are actually exercised and either
    realize or delete them — no dead "TODO" branches in the layer the memory model
    will mutate.
  - IR invariants (`crates/ir/src/invariants.rs`) are structural-only today
    (reg-defined-before-use, terminator present, targets exist). Add the stronger
    checks a future ownership/Perceus pass must be able to *assume and preserve*:
    no `Unreachable` survives in completed IR, CFG predecessor consistency, every
    heap alloc has a well-formed lifecycle. These invariants are the contract that
    makes later passes safe to write.
- **F5.3 `arch`** — **Confirm the IR is single-source-of-truth for both future
  consumers.** The parity harness itself is already covered by F2.3; the remaining
  foundation task is to ensure nothing downstream of the IR re-derives semantics
  that a second backend would have to duplicate (audit `crates/vm` for logic that
  belongs in the F5.1 runtime crate). The goal: the IR + runtime crate are the
  *only* places semantics live, so adding a backend later is translation, not
  reinvention.
- **F5.4 `memory-model` `docs-sync`** — **Document the pass insertion point.** The
  three calling conventions are already parsed and threaded through HIR
  (`crates/lower/src/lowering/item.rs:94`, `crates/lower/src/hir_types/mod.rs:27`)
  but never read by any analysis. Don't build the checker (feature) — instead
  *document precisely where* the ownership/exclusivity and Perceus passes will hook
  into the IR pipeline, and verify the data they need (conventions, spans from F3,
  `Deinit` registrations from typeck) is actually present and reachable at that
  point. This turns "the memory model is absent" from a cliff into a prepared seam.

---

## What this foundation unblocks (deferred feature work)

**Out of scope for this plan** — listed only to make the boundary explicit. Each
becomes straightforward to implement *because* Phases 0–5 closed the gaps it would
otherwise trip over.

- **`control-flow` / `closures`** — implementing iterator loops and first-class
  closures (the spiked §8.2 capture model). Unblocked by F4 (clean slate, no
  half-wiring), F3 (real spans), F2.1 (output oracle to prove iteration).
- **`backend`** — the Cranelift AOT native backend + `axiom build`
  (`crates/cli/src/lib.rs:53` is a stub). Unblocked by F5.1 (shared runtime to
  link), F5.2 (validated IR), F2.3 (parity harness ready).
- **`memory-model`** — the v1 identity: the exclusivity/ownership pass (§4.3/§4.4),
  Perceus refcounting + reuse/FBIP (§4.5/§4.6), runtime `Deinit` + drop ordering
  (§4.9; the VM heap `refcount` field at `crates/vm/src/lib.rs:36` is currently
  dead). Unblocked by F5.2/F5.4 and the whole test/diagnostics net.
- **`stdlib` / language surface** — more collections + iterator adapters, `format`
  width/precision + user `Display` (open-question #7 remainder), concurrency (§9),
  richer generics (§14 v2). Unblocked once the above exist.

---

## Dependency ordering at a glance

All items below are **foundation gap-closing only**. The deferred feature work
(memory model, Cranelift, closures, iterator loops, stdlib growth) sits *after*
this graph and is intentionally not shown as a node.

```
F0 (green gate)  ──►  F1 (docs match code)  ──►  F2 (test oracle + parity skeleton)
                                                     │
                            ┌────────────────────────┼────────────────────────┐
                            ▼                         ▼                         ▼
                   F3 (real spans)        F4 (de-trap half-built        F5.1 (runtime crate
                            │               surface: loops, closures)         boundary refactor)
                            │                         │                         │
                            └─────────────────────────┼─────────────────────────┘
                                                       ▼
                                          F5.2 (IR loose ends + invariants)
                                                       │
                                          F5.3 (parity harness scaffolding)
                                          F5.4 (document pass insertion seam)
                                                       │
                                                       ▼
                              ════ foundation solid — feature work can now begin ════
```

F3, F4, and F5.1 have no hard interdependency and can proceed in parallel once the
test oracle (F2) exists. They converge on F5.2 (a validated, fully-wired IR with
strong invariants) — the single most important deliverable, because it is the layer
every future backend and every future memory-model pass builds on. Nothing here
implements a spec feature; it removes the issues that would otherwise turn that
implementation into a bottleneck.
