# Unifying stdlib Loading — one embedded source, one pipeline

> **Status:** In progress. Prerequisite for
> [`builtin-to-stdlib-migration.md`](builtin-to-stdlib-migration.md) (its P1). Collapses the
> **four** divergent stdlib-loading behaviors into **one** loader + **one** compile pipeline,
> with a deliberate, labeled bare mode for compiler-isolation unit tests.
>
> **Companion docs:** [`extern-buffers-and-path-unification.md`](extern-buffers-and-path-unification.md)
> §1 (first identified the divergent-paths root cause), [`io-design.md`](io-design.md),
> `DESIGN_SPEC.md` §10 (modules), §11 (stdlib surface).

---

## 1. The problem — four divergent paths

`check()` **always** registers the compiler built-ins (`collect_pass` →
`register_builtin_traits/impls/methods` + `inject_prelude_sigs`), regardless of whether any
stdlib is loaded. On top of that, the stdlib source reaches the compiler **four different
ways** depending on how it is invoked:

| Path | Used by | Bodies? | Modules? | Mechanism |
|---|---|---|---|---|
| **A** concat | typeck `check_source_with_stdlib`, VM golden + `format_e2e` | ✅ | ❌ flat | `typeck::with_stdlib` — **hardcoded `include_str!` list** |
| **B** module graph | CLI dir mode (`run_check_dir`/`run_run_dir`) | ✅ | ✅ real | `discover` + `discover_library` + `merge` |
| **C** exports-only | CLI single-file (`run_check`/`run_run`), `features` corpus | ❌ names only | ✅ | `build_stdlib_exports` → `compile_source(src, exports)` |
| **D** bare | ~80 typeck unit tests (`check_source`) | ❌ | ❌ | no stdlib at all — built-ins only |

**Plus a fifth, subtler divergence in name resolution itself:** the implicit `io` prelude
(de-facto `use io::*`) is injected only in the single-file `resolve` path (Pass 1.25,
`resolve/mod.rs`), **not** in the multi-module `resolve_with_globals`. So `print` auto-resolves
single-file but would need an explicit `use` in dir mode.

### Why this is a problem
- Divergence = *two paths that both accept the same program but interpret it differently.*
  This is what hid real bugs (the `&[U8]` extern mismatch; the generic `print` stand-in).
- Path A's `include_str!` list **drifts** from disk (a new stdlib file is invisible to it).
- Paths C and D have **no bodies**, which is the only reason the `register_builtin_*` and
  `inject_prelude_sigs` stand-ins must exist — and that is what **blocks the
  builtin→stdlib migration** (you cannot delete a built-in while bare tests rely on it).

## 2. The fix — one source, one pipeline, one labeled exception

1. **One embedded stdlib source (Question 1 → Option A: `build.rs`).** A new leaf crate
   `axiom-stdlib` carries a `build.rs` that walks `stdlib/**/*.ax` at compile time and emits
   an embedded `STDLIB: &[(&str /*module path*/, &str /*source*/)]`. No hardcoded list, no
   runtime disk dependency, drift-proof (the walk takes whatever exists). Mirrors the Oxy
   `symbols.rs` single-source-of-truth pattern + `ENFORCEMENT.md`’s anti-drift rule.
2. **One compile pipeline.** A pure driver `axiom_typeck::check_modules(&[(name, source)])
   -> Thir` lifts CLI’s `lower_all_modules → build_global_exports → resolve_all_modules →
   combine → check` into the typeck crate (needs only parser + hir, both already deps).
   Every non-bare path becomes `check_modules(STDLIB ++ user_modules)`:
   - single file → `user_modules = [("", src)]` (empty module name keeps IR fn-qualification
     identical to today’s `lower` path, so VM goldens stay stable),
   - project dir → `user_modules` = discovered user graph (stdlib no longer disk-discovered),
   - stdlib tests → identical to single file.
3. **One labeled bare mode (Question 2 → Option B).** `check_source(src)` stays, but is
   redefined as `check_modules(&[("", src)])` — i.e. *the same pipeline with an empty stdlib
   input*, not a separate path. Used only for compiler-isolation unit tests + the floor
   built-ins that legitimately stay. Divergence is structurally impossible: see §4.
4. **Unify the resolve prelude.** Add the Pass 1.25 implicit-`io` injection to
   `resolve_with_globals` (shared helper) so every path resolves `print` the same way.

### Layering (acyclic)
```
parser / hir ──> axiom-typeck    (check_modules, check_source [bare], check[raw Hir])
                      ▲
                 axiom-stdlib     (build.rs embed; modules(); check_with_stdlib(src)->Thir)
                      ▲
                 axiom-cli        (single-file + dir, both via check_modules)
   axiom-stdlib is also a dev-dependency of axiom-typeck tests + axiom-vm tests.
```
`axiom-stdlib` depends on `axiom-typeck` (to offer `check_with_stdlib`); typeck does **not**
depend on `axiom-stdlib` — no cycle, and the type checker stays stdlib-agnostic.

## 3. The bare-mode guarantee (why Option B can’t re-introduce divergence)

The old divergence had two root causes: **separate code paths** and **fake stand-ins**. This
design removes both:
1. **Not a second path.** `check_source` and `check_with_stdlib` both call the *one*
   `check_modules`; they differ only by an *inert input* (empty vs. embedded stdlib list).
   There is no second resolver/checker to drift against. Enforced: exactly one `check_modules`
   entry point (grep/test guard).
2. **Nothing to diverge about.** After the migration removes the stand-ins, a bare program
   referencing `List`/`print` **fails to resolve** (no fake substituted). So the two modes can
   never both *accept* a stdlib-using program and disagree — bare rejects it, full accepts it.
   Stdlib-free programs touch no stdlib, so the stdlib modules are unused input and compile
   byte-identically. The Tier-2 floor (`Int`, `format`, …) is registered inside `check()`
   itself, shared identically by both modes.

## 4. Work plan (each step ≈ one commit; TDD; gate must pass)

- [ ] **S1 — `axiom-stdlib` crate (embed + drift guard).** New leaf crate; `build.rs` walks
      `stdlib/`, emits `STDLIB`; `pub fn modules() -> &'static [(&'static str,&'static str)]`;
      module-name derivation mirrors `discover_library`. Drift test: `modules()` name-set ==
      `discover_library(stdlib/)` name-set (dev-dep on `axiom-modules`). Workspace member +
      `[lints] workspace = true` + README. *(No behavior change yet — additive.)*
- [ ] **S2 — `check_modules` driver + unified resolve prelude.** Lift the multi-module
      pipeline into `axiom_typeck::check_modules`. Add the implicit-`io` injection to
      `resolve_with_globals`. Redefine bare `check_source` over `check_modules`. Regenerate
      any shifted bare `.thir` goldens (verify shifts are resolution-equivalent, not regressions).
- [ ] **S3 — stdlib-backed convenience; retire Path A + `with_stdlib`.** `axiom-stdlib`
      gains `check_with_stdlib(src) -> Thir = check_modules(modules() ++ [("", src)])`. Point
      typeck tests + VM tests at it (add dev-deps). Delete `typeck::with_stdlib`,
      `check_source_with_stdlib`, `stdlib.rs`. Regen VM/typeck goldens.
- [ ] **S4 — CLI single-file + dir via `check_modules`; retire Path C.** Single-file:
      `check_modules(modules() ++ [("", src)])` → IR/VM. Dir: discover *user* graph →
      `check_modules(modules() ++ user_modules)`. Delete `build_stdlib_exports`, the
      exports-only `compile_source` signature, and the `discover_library` stdlib merge. Regen
      `features` corpus goldens.
- [ ] **S5 — docs + spec.** Mark this doc complete; update `builtin-to-stdlib-migration.md`
      P1, `extern-buffers-and-path-unification.md` §1, `io-design.md`; note the embedded-stdlib
      + single-pipeline architecture in `DESIGN_SPEC.md` §11. Update per-crate READMEs.

## 5. Risks / watch-items
- **Golden churn (S2/S3/S4).** Switching bare tests from `lower` to
  `lower_structural + resolve_with_globals` may shift HirIds/THIR. Verify each shift is
  resolution-equivalent before accepting `UPDATE_SNAPSHOTS`.
- **IR fn-name stability.** The synthetic user module uses name `""` so `module_path`-based
  IR qualification matches today’s `lower` output → VM `.trace` goldens stay stable.
- **Prelude scope creep.** Pass 1.25 currently injects only `io`. Keep it to `io` now; the
  broader `core::*` prelude is part of the migration, not this change.

## 6. Out of scope (deferred)
| Item | Why |
|---|---|
| P0 stdlib **relocation** (`io.ax`→`std/io.ax`, `collections/`→`std/collections/`) | Tracked in the migration doc; the embed walks whatever layout exists, so it is independent of this unification. |
| Lazy parsing / DCE of unused stdlib | Pure optimization; at v0 scale parsing a few small files is free. |
| Removing the `register_builtin_*` stand-ins | That **is** the migration (this doc only unblocks it). |
