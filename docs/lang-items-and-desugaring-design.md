# Lang Items & Desugaring — Design Plan

> **Status: [Deferred implementation; design decisions captured].** Nothing here is built yet. This doc records *the plan* so
> the work is not forgotten and lands consistently when the trigger fires (§7). The
> current behaviour (list literals desugar in IR via hardcoded `"List"` strings) is
> **correct and shipped** — this is about paying down a known coupling cleanly, not
> fixing a bug.

## 0. The concern this answers

A handful of language features are *defined* in terms of specific standard-library
types: `[a, b, c]` means `List`, `["k": v]` will mean `Map`, `for x in xs` will mean
`Iterator`, `?`/`try` mean `Option`/`Result`. That compiler→stdlib link is legitimate
and unavoidable — the language deliberately welds this syntax to those types.

The concern is **how that link is expressed.** Today it is:

- **hardcoded name strings** scattered across stages (`"List"`, `"List::push"`,
  `"List::with_capacity"`, `"List::new"`, `"<Type>::subscript"`), matched by spelling, and
- a **fake `def_id: HirId(0)`** standing in for the real `List` definition.

So the compiler identifies the same type two ways (a name string *and* a placeholder
id), in several places, with no link to the actual `struct List` in the stdlib. It works
only because every later stage re-finds `List` by name. As more sugar lands, this
brittleness compounds. This doc plans the clean binding layer (**lang items**, §3.3) and
the **desugaring stage** (§4), plus the drift-guard **harness** (§6) that keeps both
honest — mirroring `lexer-testing.md` / `collection-type-design.md` §8.

## 1. Lineage (this is acknowledged debt, not a discovery)

The `builtin_types` registry and the hardcoded `List`/`Map` methods were already removed
during the struct-v0 migration. `struct-v0-plan.md` §"Implemented" states the residue
explicitly: *"Only `push`/`set` intrinsics and `infer_list_lit` retain List/Map strings —
these are native operations that can't be expressed in library code yet."* The M6/M7
stdlib migration then moved the *methods* into `.ax` (see `builtin-to-stdlib-migration.md`),
and the list-literal lowering was moved off the old `IrInstr::ListNew` onto `with_capacity`
+ `push`. This doc is the **last mile**: removing the remaining name/`def_id` hardcoding
and giving desugaring a principled home.

## 2. Current state — every coupling site, honestly

| # | Coupling | Where | Form |
|---|---|---|---|
| C1 | literal element-type → `List<T>` | `axiom-typeck/.../typeck/methods.rs` `infer_list_lit` (~487) | `name: "List"`, **`def_id: HirId(0)`** (fake) |
| C2 | (other) instance built with placeholder id | `methods.rs:283` | **`def_id: HirId(0)`** |
| C3 | `[a,b,c]` → constructor + pushes | `axiom-ir/.../lower/expr.rs` `lower_list_lit` (~272) | `"List::with_capacity"`, `"List::push"`, `"List::new"` |
| C4 | `base[i]` → subscript call | `lower/expr.rs` `lower_index` (~235) | `format!("{type_name}::subscript")` (dynamic, name-based) |
| C5 | which names are builtin/extern | `axiom-vm/.../exec/builtins.rs` `is_builtin`/`resolve_extern` | name match-lists |

> C4/C5 are *convention*-based name dispatch (the VM is name-keyed by design), not the
> same fake-`def_id` smell as C1/C2. They are listed so the inventory is complete; the
> lang-item work (§3.3) primarily targets C1–C3.

## 3. The name-coupling cleanup — three steps, cheapest → cleanest

### 3.1 Step 1 — Centralize the well-known names (single source of truth) — [Done]

Every magic name now lives in **one** module — `axiom-hir/src/lang.rs` — as named
constants:

```rust
pub const LIST: &str = "List";
pub const LIST_NEW: &str = "List::new";
pub const LIST_WITH_CAPACITY: &str = "List::with_capacity";
pub const LIST_PUSH: &str = "List::push";
pub const SUBSCRIPT: &str = "subscript";
```

typeck and IR lowering reference these instead of inline string literals. The coupling
still exists, but it is **one greppable place** rather than scattered spellings. This is
the `symbols.rs` single-source-of-truth discipline the lexer already follows
(`lexer-testing.md` §5.2), applied to stdlib names. **Done:** the constants live in
`axiom-hir/src/lang.rs`; `infer_list_lit` (typeck) and `lower_list_lit`/`lower_index`/
`lower_subscript` (IR) reference them; a source-scan drift test (`lang::tests`, the
§6.2 "no raw stdlib-name strings" guard) fails the build if a qualified `List::…` string
reappears outside the module.

### 3.2 Step 2 — Resolve a *real* `def_id` (kill the `HirId(0)` lie) — [Done]

After name resolution, the multi-module driver (`check_modules`) collects every
`@lang("…")` binding from the **stdlib** HIR and resolves each required lang item to its
**real `DefId`** (`axiom_hir::resolve_lang_items` → `LangItems`). `infer_list_lit` and
`build_impl_self_pattern` now stamp the list type's `Ty::Instance` with the true id
(falling back to the inert `HirId(0)` only in the deliberate no-stdlib test mode, where
the field is never read anyway). Benefits, realized:

- removes the placeholder `HirId(0)` for the list type (C1, C2);
- a **missing/duplicate `List`** binding becomes a *defined compiler diagnostic at a single
  point* (`MissingLangItem`/`DuplicateLangItem`) instead of a silent wrong pointer.

### 3.3 Step 3 — Lang items: tag the binding in the stdlib — [Done]

The stdlib type declares *itself* as the one the compiler should use, via an `@lang("…")`
attribute (lexed as `Punct::At`, parsed as an `Attr`/`AttrList` node, lowered to a
`lang_tag` on the HIR `StructDef`/`FnDef`):

```axiom
@lang("list")
struct List<T: Deinit> { ... }

@lang("list_new")
fn new() -> List<T> { ... }

@lang("list_with_capacity")
fn with_capacity(capacity: Int) -> List<T> { ... }

@lang("list_push")
fn push(inout self, sink element: T) { ... }
```

The compiler resolves "the def tagged `list`" rather than matching the string `"List"`.
The name is no longer load-bearing — rename `List` → `Vector` and, as long as the tag moves
with it, `[...]` keeps working. `@lang` tags are honored **only inside the stdlib**; a tag in
user code is rejected (`LangItemOutsideStdlib`) so user code can't hijack a lang item.

Registry shape (`axiom_hir::LangItems`): `{ list, list_new, list_with_capacity, list_push }`
as `Option<DefId>`, populated once after resolution, read by typeck (and, next, IR lowering
and the desugar pass). New keys join `REQUIRED_LANG_ITEMS`; a required key with no stdlib
binding fails the consistency check.

## 4. The desugaring stage — where should sugar expand? — [Decided: interim] / [Deferred: end-state]

**The tension, stated honestly — and it is mild.** `v0-roadmap.md` (M1) calls HIR the
*"desugared, ID-keyed"* layer. That word **"desugared" is ambiguous**, and the difference
decides whether anything is wrong:

- **Loose sense (almost certainly what M1 meant):** HIR is a *lowered, trivia-stripped,
  ID-assigned, name-resolved* tree — a simplified representation of the CST/AST. HIR **is**
  that. M1's actual deliverable was "name-resolved tree + resolution diagnostics + drift
  guard" — **no sugar-expansion transforms**, and `for`-loops are kept structural to this
  day. Under this (intended) reading, putting list-literal *sugar expansion* in IR
  **violates nothing.**
- **Strict sense (sugar constructs rewritten into core forms):** HIR does **none** of this
  today. *If* the project later adopts this stricter role for HIR, then the IR placement
  becomes interim debt to consolidate — a future *preference*, not a past error.

So no decision was broken: the one desugaring we have (list literals) lives in **IR
lowering** as a sound pragmatic choice, and the shipped behaviour is correct.

- **[Decided, interim]:** keep list-literal desugaring in IR *for now*. It is the only
  desugaring, IR already synthesizes calls routinely, and a lone HIR special-case would be
  odd. This is consistent **because it is the only one.**
- **[Deferred, end-state]:** when there are ~3–4 desugarings (§5), **adopt the stricter
  HIR-as-desugar role** — a dedicated **desugar pass on HIR**, after name resolution,
  before typeck. At that point HIR-level becomes the *more* principled home because:
  - desugared output goes through normal type-checking/resolution (no string-keyed
    method names synthesized late; no `infer_list_lit` special-case — `[a,b,c]` becomes a
    real `with_capacity(n)` + `push` call chain that types itself, including element
    unification via `push`'s `T`);
  - one place owns "syntax sugar → core", instead of logic spread across IR `lower_*`.

**Decision rule (the trigger):** introduce the HIR desugar pass when the *second or third*
literal/sugar form appears (map/set literals, `for`→iterator). Until then, IR placement is
interim debt — tracked here, not forgotten.

> Cross-check: this is the **same trigger** as the lang-items build (§3.3) and they should
> land together — a HIR desugar pass that emits real calls *needs* resolved lang-item
> `def_id`s to point those calls at. Sequence: §3.1 → §3.2/§3.3 → §4 pass.

## 5. The full backlog — every present & anticipated coupling/sugar (so nothing's forgotten)

| Sugar / coupling | Couples to | Current state | Target |
|---|---|---|---|
| `[a, b, c]` list literal | `List` | **done** — IR desugar to `with_capacity`+`push` | HIR desugar + lang item |
| `[]` empty literal | `List` | **`NotYetSupported`** in `infer_list_lit` | lower to `List::new()` once typed by annotation |
| `["k": v]` map literal | `Map` | not implemented (`collection-type-design.md` §6.2) | HIR desugar + lang item |
| `{1, 2, 3}` set literal | `Set` | not implemented; grammar ambiguity w/ blocks (open) | HIR desugar + lang item |
| `for x in xs { }` | `Iterator` | loops kept structural in HIR | desugar to `next()` loop + `Iterator` lang item |
| `a..b` range literal | `Range` | not implemented | lang item |
| `?` / `try` | `Option` / `Result` | error-handling sugar (`DESIGN_SPEC.md` §6.5/§6.4) | lang items when wired |
| `base[i]` subscript | `<T>::subscript` | name-convention dispatch (C4) | keep convention, or formalise as lang trait method |
| compound assign `+=` etc. | operator traits | single mechanism (`DESIGN_SPEC`) | desugar candidate |
| import gating not enforced | stdlib visibility | `List`/`Map` resolve without `use` (`modules-design.md`) | **related** — lang-item binding and user prelude/import visibility must stay separate decisions |

## 6. Harness & constraints (the drift guards) — mirroring the other layers

Same philosophy as `lexer-testing.md` §4/§9 and `collection-type-design.md` §8:
**mechanize the "can't silently drift" guard; be honest about the soft residue.**

### 6.1 The six layers (applied)
1. **Unit** — the lang-item registry builder; the desugar transform as a pure function
   (HIR→HIR) testable on hand-built nodes.
2. **Golden snapshots** — pin the *exact* desugared output. e.g. a `.ax` fixture
   `list_literal.ax` whose HIR/IR golden shows `[10,20,30]` → `with_capacity(3)` + 3×`push`
   (the existing `multi_file_golden` HIR snapshots are the template; the
   `test_list_literal_preallocates_exact_capacity` e2e is the behavioural template).
3. **Coverage invariants / drift guards** — §6.2, §6.3 below (the core fear: "a case I
   never imagined slipped through").
4. **e2e** — `.ax` → VM, asserting the *rendered* result (not trace substrings — see the
   prefix lesson in `list_e2e.rs`).
5. **Diagnostics** — snapshot the new errors ("required lang item `list` missing").
6. **Fuzz/property** (as relevant) — desugar then re-typecheck must not change a program's
   type or introduce diagnostics on happy-path inputs.

### 6.2 Lang-item registry completeness (the `symbol_consistency` analogue) — **Hard**
A consistency `#[test]` (and a build-time check) asserting:
- **every** compiler-required lang item resolves to **exactly one** stdlib def (no
  missing, no duplicate) — adding a required item without a stdlib binding **fails the
  build**, the way an unnamed `TokenKind` fails the lexer's symbol-consistency test;
- **no orphan tags** — a `@lang("…")` in stdlib with no compiler consumer fails too.
This is what turns the `HirId(0)` *lie* into a *guaranteed* truth.

### 6.3 Desugaring coverage invariant (the drift guard) — **Hard**
A single exhaustiveness check (the `IrInstr`/`Terminator` variant-coverage tests in
`axiom-vm/tests/invariants.rs` are the template): **every sugar `Expr` variant has a
desugaring rule and a golden fixture.** Adding a new literal/sugar `Expr` variant without
both **fails**. Prevents a new syntax silently falling through to "no rule / wrong rule".

### 6.4 The "no compiler-native value" invariant — **Hard**
Assert structurally that a desugared literal produces a **real stdlib struct**, never a
compiler-private runtime value (this is the regression that `IrInstr::ListNew` /
`Value::List` were — they let `.count()` silently return `()`). The capacity test
(`cap == n`, not `n` rounded up by growth) is the behavioural proxy; pair with a golden
that shows the call chain.

### 6.5 What's mechanically enforced vs judgment

| Guarantee | Mechanism | Strength |
|---|---|---|
| Every required lang item bound to exactly one stdlib def | registry consistency `#[test]` + build check | **Hard** |
| No orphan `@lang` tags | same consistency test | **Hard** |
| Every sugar `Expr` variant has a desugaring + golden | coverage invariant `#[test]` | **Hard** |
| Desugared output is exact/stable | HIR/IR golden snapshots | **Hard** |
| Literal yields a real stdlib struct (no native value) | structural assert + capacity golden | **Hard** |
| Missing lang item → clear error, not silent miscompile | diagnostic snapshot | **Hard** |
| No raw stdlib-name strings outside the names module | source-scan `#[test]` (lexer §5.2 pattern) | **Hard (narrow)** |
| Desugaring preserves program type/semantics | re-typecheck property test + review | **Mixed** |
| "One obvious way" / no overlapping sugar | review against `DESIGN_SPEC` | **Soft at the margin** |

## 7. Build order (TDD) — when the trigger fires

1. **§3.1 names module** + the source-scan + consistency scaffolding (registry can start
   string-keyed). Green before behaviour changes. ✅ **Done** — `axiom-hir/src/lang.rs` +
   `lang::tests` source-scan drift guard.
2. **§3.2/§3.3 lang-item registry** — resolve real `def_id`s; consistency test (§6.2) goes
   green; `infer_list_lit` switches off `HirId(0)`. *No desugaring moved yet.* ✅ **Done** —
   `@lang` attribute + `axiom_hir::LangItems`; `tests/lang_items.rs` consistency + policy.
3. **§6.3 coverage invariant** + goldens for the *existing* IR desugaring — lock current
   behaviour before relocating it. ✅ **Done** — `axiom-ir/tests/desugar_coverage.rs`
   (Expr-variant drift guard + per-sugar golden + desugared-calls check) and
   `tests/desugar_goldens/list_literal.ir`.
4. **§4 HIR desugar pass** — introduce the pass; move list-literal desugaring HIR-ward
   emitting real (now lang-item-resolved) calls; delete the IR `lower_list_lit` synthesis
   and the `infer_list_lit` special-case; goldens must show the *same* end behaviour.
5. **Extend** to the next sugar (map/set/`for`) — each adds one desugar rule + one fixture
   + one lang item; the invariants in §6 need **zero** changes if the architecture is
   data-driven (the real test of the design — `lexer-testing.md` §5.3).

## 8. Decisions now vs remaining open questions (mirroring `DESIGN_SPEC` §15)

### 8.1 Decisions captured by this doc (not open)

- **Lang-item tag syntax is the `@lang("…")` attribute** (O1, resolved) — self-describing on
  the def, honored only inside the stdlib.
- **Prelude visibility stays independent from lang-item binding.** Lang items are compiler
  binding metadata, not automatic prelude imports.
- **HIR desugar ordering:** resolve declarations + lang items first, then desugar, then
  resolve synthesized call paths before typecheck.
- **Empty `[]` support** should land with the HIR-side list-literal desugar move.

### 8.2 Remaining open questions

| # | Question | Lean |
|---|---|---|
| O3 | Is `subscript` (C4) a lang item, or does name-convention dispatch stay? | keep convention unless it bites |

## 9. Cross-references

- `DESIGN_SPEC.md` §15 (open questions), §14 (roadmap), §6.4 (`try`) and §6.5 (`?`).
- `collection-type-design.md` §6 (literal syntax), §8 (testing spec).
- `ir-design.md` (current IR `lower_list_lit`), `builtin-to-stdlib-migration.md` (M6/M7),
  `struct-v0-plan.md` (the `builtin_types` removal lineage).
- `v0-roadmap.md` M1 (HIR described as the "desugared" layer — the basis for §4).
- `modules-design.md` (O2 intersection).
- `lexer-testing.md` §4/§5/§9 and `axiom-vm/tests/invariants.rs` (harness templates).
